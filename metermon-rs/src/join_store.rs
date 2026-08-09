//! redb-backed [`JoinStore`] — durable LoRaWAN join state for 1.0.4 anti-replay.
//!
//! Persists, per DevEUI, the highest accepted DevNonce and the next JoinNonce, so a
//! gateway restart cannot reopen the DevNonce replay window or send a JoinNonce that
//! regresses (which a strict 1.0.4 device would reject). `admit_join` performs the
//! freshness check, the DevNonce record, and the JoinNonce reservation in **one
//! committed write transaction**, so a crash cannot advance one without the other,
//! and returns only after the commit — the durable-before-live guarantee the
//! responder relies on before it transmits a JoinAccept. See
//! docs/design/lorawan-join-persistence.md.

use mbus_rs::lorawan::{
    admit_dev_nonce, DevNonceVerdict, JoinAdmission, JoinStore, JoinStoreError,
};
use redb::{Database, ReadableDatabase, TableDefinition};
use std::path::Path;
use std::sync::Arc;

/// Per-device join state: `dev_eui` hex (matching the store's other string keys) ->
/// JSON `{"last_dev_nonce":u16,"next_join_nonce":u32}`.
const JOIN_STATE: TableDefinition<&str, &str> = TableDefinition::new("join_state");

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
struct JoinRecord {
    last_dev_nonce: u16,
    /// The JoinNonce to hand out next; starts at 1. Only ever increases.
    next_join_nonce: u32,
    /// Whether this device has ever been admitted (distinguishes "DevNonce 0 seen"
    /// from "never seen", which the freshness rule must not conflate).
    seen: bool,
}

pub struct RedbJoinStore {
    db: Arc<Database>,
}

fn err<E: std::fmt::Display>(e: E) -> JoinStoreError {
    JoinStoreError(e.to_string())
}

fn key(dev_eui: &[u8; 8]) -> String {
    hex::encode(dev_eui)
}

impl RedbJoinStore {
    /// Open (creating if absent) the join-state store. Shares a database file with
    /// the device store when given the same path — redb tables are independent.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, JoinStoreError> {
        let db = Database::create(path).map_err(err)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Wrap an already-open database (e.g. the device store's), so join state lives
    /// in the same file under its own table.
    pub fn from_db(db: Arc<Database>) -> Self {
        Self { db }
    }

    fn read(&self, dev_eui: &[u8; 8]) -> Result<Option<JoinRecord>, JoinStoreError> {
        let r = self.db.begin_read().map_err(err)?;
        let t = match r.open_table(JOIN_STATE) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(err(e)),
        };
        match t.get(key(dev_eui).as_str()).map_err(err)? {
            Some(v) => serde_json::from_str(v.value()).map(Some).map_err(err),
            None => Ok(None),
        }
    }
}

impl JoinStore for RedbJoinStore {
    fn admit_join(
        &mut self,
        dev_eui: &[u8; 8],
        dev_nonce: u16,
    ) -> Result<JoinAdmission, JoinStoreError> {
        let existing = self.read(dev_eui)?;
        let last = existing.filter(|r| r.seen).map(|r| r.last_dev_nonce);
        if let DevNonceVerdict::Replay { last, seen } = admit_dev_nonce(last, dev_nonce) {
            return Ok(JoinAdmission::Replay { last, seen });
        }

        // Fresh: record the DevNonce and reserve the next JoinNonce in one commit.
        let mut rec = existing.unwrap_or_default();
        if rec.next_join_nonce == 0 {
            rec.next_join_nonce = 1; // first JoinNonce is 1, never 0
        }
        let join_nonce = rec.next_join_nonce;
        rec.next_join_nonce = join_nonce.wrapping_add(1) & 0x00FF_FFFF;
        rec.last_dev_nonce = dev_nonce;
        rec.seen = true;

        let json = serde_json::to_string(&rec).map_err(err)?;
        let w = self.db.begin_write().map_err(err)?;
        {
            let mut t = w.open_table(JOIN_STATE).map_err(err)?;
            t.insert(key(dev_eui).as_str(), json.as_str())
                .map_err(err)?;
        }
        w.commit().map_err(err)?; // durable before we return, hence before transmit
        Ok(JoinAdmission::Admitted { join_nonce })
    }

    fn last_dev_nonce(&self, dev_eui: &[u8; 8]) -> Option<u16> {
        self.read(dev_eui)
            .ok()
            .flatten()
            .filter(|r| r.seen)
            .map(|r| r.last_dev_nonce)
    }

    fn reset_dev_nonce(&mut self, dev_eui: &[u8; 8]) -> Result<(), JoinStoreError> {
        // Clear the DevNonce high-water but keep next_join_nonce advancing — a
        // re-provisioned device must still never see a JoinNonce go backwards.
        let existing = self.read(dev_eui)?;
        let rec = JoinRecord {
            last_dev_nonce: 0,
            next_join_nonce: existing.map(|r| r.next_join_nonce.max(1)).unwrap_or(1),
            seen: false,
        };
        let json = serde_json::to_string(&rec).map_err(err)?;
        let w = self.db.begin_write().map_err(err)?;
        {
            let mut t = w.open_table(JOIN_STATE).map_err(err)?;
            t.insert(key(dev_eui).as_str(), json.as_str())
                .map_err(err)?;
        }
        w.commit().map_err(err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> std::path::PathBuf {
        // Distinct file per test; no Date/rand available, so name them explicitly.
        let mut p = std::env::temp_dir();
        p.push(format!("mbus_join_test_{name}.redb"));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn persists_across_reopen_and_advances_join_nonce() {
        let path = temp_db("reopen");
        let eui = [0x00, 0x04, 0xA3, 0x0B, 0x00, 0xFF, 0x00, 0x01];

        // First lifetime: two joins.
        {
            let mut s = RedbJoinStore::open(&path).unwrap();
            assert_eq!(
                s.admit_join(&eui, 0).unwrap(),
                JoinAdmission::Admitted { join_nonce: 1 }
            );
            assert_eq!(
                s.admit_join(&eui, 1).unwrap(),
                JoinAdmission::Admitted { join_nonce: 2 }
            );
        } // store dropped — simulates the gateway restart

        // Second lifetime: the reboot the live restart exposed. JoinNonce must
        // continue at 3, NOT reset to 1, and the DevNonce high-water must survive.
        {
            let mut s = RedbJoinStore::open(&path).unwrap();
            assert_eq!(s.last_dev_nonce(&eui), Some(1));
            assert_eq!(
                s.admit_join(&eui, 1).unwrap(),
                JoinAdmission::Replay { last: 1, seen: 1 },
                "a DevNonce replayed across the restart must still be rejected"
            );
            assert_eq!(
                s.admit_join(&eui, 2).unwrap(),
                JoinAdmission::Admitted { join_nonce: 3 },
                "JoinNonce must not regress across a restart"
            );
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reset_clears_dev_nonce_but_keeps_join_nonce_monotonic() {
        let path = temp_db("reset");
        let eui = [9u8; 8];
        let mut s = RedbJoinStore::open(&path).unwrap();
        s.admit_join(&eui, 50).unwrap(); // join_nonce 1
        s.admit_join(&eui, 51).unwrap(); // join_nonce 2
        s.reset_dev_nonce(&eui).unwrap();
        // DevNonce 0 now accepted again (re-provision)...
        let adm = s.admit_join(&eui, 0).unwrap();
        // ...but the JoinNonce keeps climbing (must never go backwards).
        assert_eq!(adm, JoinAdmission::Admitted { join_nonce: 3 });
        let _ = std::fs::remove_file(&path);
    }
}
