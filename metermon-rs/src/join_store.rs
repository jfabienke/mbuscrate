//! redb-backed [`JoinStore`] — durable LoRaWAN join state for DevNonce anti-replay.
//!
//! Persists, per DevEUI, the window of recently-accepted DevNonces (the 1.0.2 fleet
//! draws them randomly, so anti-replay is set-membership, not a high-water counter —
//! see [`DevNoncePolicy`]) and the next JoinNonce, so a gateway restart cannot reopen
//! the DevNonce replay window or send a JoinNonce that regresses (which a strict
//! device would reject). `admit_join` performs the freshness check, the DevNonce
//! record, and the JoinNonce reservation in **one committed write transaction**, so a
//! crash cannot advance one without the other, and returns only after the commit —
//! the durable-before-live guarantee the responder relies on before it transmits a
//! JoinAccept. See docs/design/lorawan-join-persistence.md.

use mbus_rs::lorawan::{
    admit_dev_nonce, admit_dev_nonce_windowed, DevAddrAssignment, DevNoncePolicy, DevNonceVerdict,
    JoinAdmission, JoinStore, JoinStoreError, DEV_ADDR_BASE,
};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::Path;
use std::sync::Arc;

/// Per-device join state: `dev_eui` hex (matching the store's other string keys) ->
/// JSON `{"last_dev_nonce":u16,"next_join_nonce":u32,"seen":bool,"recent_nonces":[u16]}`.
const JOIN_STATE: TableDefinition<&str, &str> = TableDefinition::new("join_state");

/// DevAddr allocator high-water: the single key `"next"` -> the next address to hand
/// out. Its own table rather than a reserved key in [`JOIN_STATE`], so it can never be
/// confused with a DevEUI-keyed [`JoinRecord`].
const DEV_ADDR_ALLOC: TableDefinition<&str, u64> = TableDefinition::new("dev_addr_alloc");
const ALLOC_KEY: &str = "next";

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct JoinRecord {
    last_dev_nonce: u16,
    /// The JoinNonce to hand out next; starts at 1. Only ever increases.
    next_join_nonce: u32,
    /// Whether this device has ever been admitted (distinguishes "DevNonce 0 seen"
    /// from "never seen", which the freshness rule must not conflate).
    seen: bool,
    /// Window of recently-accepted DevNonces, for the 1.0.2 [`DevNoncePolicy::RandomWindow`]
    /// rule. `#[serde(default)]` so records written before this field existed load as
    /// an empty window and get seeded from `last_dev_nonce` on their next join.
    #[serde(default)]
    recent_nonces: Vec<u16>,
    /// The DevAddr allocated to this device, stable across rejoins. `#[serde(default)]`
    /// so records written before the allocator existed load as unallocated and get an
    /// address on their next join.
    #[serde(default)]
    dev_addr: Option<u32>,
}

pub struct RedbJoinStore {
    db: Arc<Database>,
    policy: DevNoncePolicy,
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
    /// Uses the default anti-replay policy ([`DevNoncePolicy::RandomWindow`], correct
    /// for the 1.0.2 fleet).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, JoinStoreError> {
        Self::open_with_policy(path, DevNoncePolicy::default())
    }

    /// As [`open`](Self::open) but with an explicit anti-replay policy (e.g.
    /// [`DevNoncePolicy::Counter`] for a strictly-1.0.4 device).
    pub fn open_with_policy(
        path: impl AsRef<Path>,
        policy: DevNoncePolicy,
    ) -> Result<Self, JoinStoreError> {
        let db = Database::create(path).map_err(err)?;
        Ok(Self {
            db: Arc::new(db),
            policy,
        })
    }

    /// Wrap an already-open database (e.g. the device store's), so join state lives
    /// in the same file under its own table. Uses the default policy.
    #[allow(dead_code)] // shared-db constructor; not wired until the join responder ships (#32)
    pub fn from_db(db: Arc<Database>) -> Self {
        Self {
            db,
            policy: DevNoncePolicy::default(),
        }
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
        let mut rec = existing.unwrap_or_default();

        // Migration: a record predating `recent_nonces` still knows its last accepted
        // value; seed the window with it so an upgrade never briefly forgets the most
        // recent nonce and re-admits an immediate replay.
        if matches!(self.policy, DevNoncePolicy::RandomWindow { .. })
            && rec.seen
            && rec.recent_nonces.is_empty()
        {
            rec.recent_nonces.push(rec.last_dev_nonce);
        }

        let last_hi = if rec.seen {
            Some(rec.last_dev_nonce)
        } else {
            None
        };
        let verdict = match self.policy {
            DevNoncePolicy::Counter => admit_dev_nonce(last_hi, dev_nonce),
            DevNoncePolicy::RandomWindow { .. } => {
                admit_dev_nonce_windowed(&rec.recent_nonces, last_hi, dev_nonce)
            }
        };
        if let DevNonceVerdict::Replay { last, seen } = verdict {
            return Ok(JoinAdmission::Replay { last, seen });
        }

        // Fresh: record the DevNonce and reserve the next JoinNonce in one commit.
        if rec.next_join_nonce == 0 {
            rec.next_join_nonce = 1; // first JoinNonce is 1, never 0
        }
        let join_nonce = rec.next_join_nonce;
        rec.next_join_nonce = join_nonce.wrapping_add(1) & 0x00FF_FFFF;
        rec.last_dev_nonce = dev_nonce;
        rec.seen = true;
        if let DevNoncePolicy::RandomWindow { keep } = self.policy {
            rec.recent_nonces.push(dev_nonce);
            if rec.recent_nonces.len() > keep {
                let excess = rec.recent_nonces.len() - keep;
                rec.recent_nonces.drain(0..excess);
            }
        }

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
            next_join_nonce: existing
                .as_ref()
                .map(|r| r.next_join_nonce.max(1))
                .unwrap_or(1),
            seen: false,
            recent_nonces: Vec::new(),
            // Keep the address: a re-provisioned device is still the same device, and
            // dropping it here would both orphan routing and leak an address.
            dev_addr: existing.and_then(|r| r.dev_addr),
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

    fn assign_dev_addr(&mut self, dev_eui: &[u8; 8]) -> Result<DevAddrAssignment, JoinStoreError> {
        let existing = self.read(dev_eui)?;
        if let Some(addr) = existing.as_ref().and_then(|r| r.dev_addr) {
            // Already allocated: the device keeps its address across rejoins, and the
            // presence of one is exactly what makes this a re-join.
            return Ok(DevAddrAssignment {
                dev_addr: addr,
                previous: Some(addr),
            });
        }

        let mut rec = existing.unwrap_or_default();
        // Allocate and record in ONE write transaction: a crash between bumping the
        // high-water and storing the device's address would otherwise either leak an
        // address or, worse, hand the same one out twice.
        let w = self.db.begin_write().map_err(err)?;
        let assigned;
        {
            let mut alloc = w.open_table(DEV_ADDR_ALLOC).map_err(err)?;
            let next = alloc
                .get(ALLOC_KEY)
                .map_err(err)?
                .map(|v| v.value() as u32)
                .unwrap_or(DEV_ADDR_BASE);
            assigned = next;
            alloc
                .insert(ALLOC_KEY, next.wrapping_add(1) as u64)
                .map_err(err)?;

            rec.dev_addr = Some(assigned);
            if rec.next_join_nonce == 0 {
                rec.next_join_nonce = 1;
            }
            let json = serde_json::to_string(&rec).map_err(err)?;
            let mut t = w.open_table(JOIN_STATE).map_err(err)?;
            t.insert(key(dev_eui).as_str(), json.as_str())
                .map_err(err)?;
        }
        w.commit().map_err(err)?; // durable before the caller transmits a JoinAccept
        Ok(DevAddrAssignment {
            dev_addr: assigned,
            previous: None,
        })
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
    fn windowed_replay_window_survives_a_restart() {
        // The 1.0.2 default: random, non-monotonic nonces are all admitted, and the
        // remembered window persists across a gateway restart so a replay stays
        // rejected — while a lower-but-fresh value (the old counter bug) is admitted.
        let path = temp_db("windowed_reopen");
        let eui = [0x00, 0x04, 0xA3, 0x0B, 0x00, 0xFF, 0x0A, 0x0B];
        {
            let mut s = RedbJoinStore::open(&path).unwrap();
            assert!(matches!(
                s.admit_join(&eui, 40_000).unwrap(),
                JoinAdmission::Admitted { .. }
            ));
            // Lower than the high-water, but never used — must be admitted (the fix).
            assert!(matches!(
                s.admit_join(&eui, 12_000).unwrap(),
                JoinAdmission::Admitted { .. }
            ));
        } // restart

        {
            let mut s = RedbJoinStore::open(&path).unwrap();
            // A value in the persisted window is still a replay after the restart...
            assert!(matches!(
                s.admit_join(&eui, 40_000).unwrap(),
                JoinAdmission::Replay { .. }
            ));
            // ...but another fresh low value is admitted.
            assert!(matches!(
                s.admit_join(&eui, 7).unwrap(),
                JoinAdmission::Admitted { .. }
            ));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn counter_policy_store_enforces_strict_increase() {
        // The opt-in 1.0.4 hardening, selectable per store.
        let path = temp_db("counter_policy");
        let eui = [0xC0u8; 8];
        let mut s = RedbJoinStore::open_with_policy(&path, DevNoncePolicy::Counter).unwrap();
        assert!(matches!(
            s.admit_join(&eui, 5).unwrap(),
            JoinAdmission::Admitted { .. }
        ));
        assert!(matches!(
            s.admit_join(&eui, 4).unwrap(),
            JoinAdmission::Replay { .. }
        ));
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

#[cfg(test)]
mod dev_addr_tests {
    use super::*;

    fn temp_db(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("mbus_devaddr_test_{name}.redb"));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn two_devices_never_share_an_address() {
        // The bug this replaces: an in-memory counter restarting per responder handed
        // 0x26000001 to every device that joined in a separate window.
        let path = temp_db("distinct");
        let mut s = RedbJoinStore::open(&path).unwrap();
        let a = s.assign_dev_addr(&[0xAA; 8]).unwrap();
        let b = s.assign_dev_addr(&[0xBB; 8]).unwrap();
        assert_eq!(a.dev_addr, DEV_ADDR_BASE);
        assert_eq!(b.dev_addr, DEV_ADDR_BASE + 1);
        assert_ne!(a.dev_addr, b.dev_addr);
        assert_eq!(a.previous, None); // both first-ever allocations
        assert_eq!(b.previous, None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_device_keeps_its_address_across_rejoins_and_reports_the_previous() {
        let path = temp_db("stable");
        let mut s = RedbJoinStore::open(&path).unwrap();
        let first = s.assign_dev_addr(&[0xCC; 8]).unwrap();
        assert_eq!(first.previous, None);
        assert!(!first.is_rejoin());

        let again = s.assign_dev_addr(&[0xCC; 8]).unwrap();
        assert_eq!(again.dev_addr, first.dev_addr, "address must be stable");
        assert_eq!(again.previous, Some(first.dev_addr));
        assert!(
            again.is_rejoin(),
            "a device that already had one is re-joining"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn allocation_survives_a_restart() {
        // The whole point: reopening must not restart the counter at the base and
        // re-issue an address already in use.
        let path = temp_db("restart");
        let first = {
            let mut s = RedbJoinStore::open(&path).unwrap();
            s.assign_dev_addr(&[0x11; 8]).unwrap().dev_addr
        };
        {
            let mut s = RedbJoinStore::open(&path).unwrap(); // "restart"
            let same = s.assign_dev_addr(&[0x11; 8]).unwrap();
            assert_eq!(same.dev_addr, first, "known device keeps its address");
            assert_eq!(same.previous, Some(first));

            let fresh = s.assign_dev_addr(&[0x22; 8]).unwrap();
            assert_ne!(fresh.dev_addr, first, "a new device must not reuse it");
            assert_eq!(fresh.dev_addr, first + 1, "high-water survived the restart");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_dev_nonce_reset_keeps_the_address() {
        // Re-provisioning a factory-reset device is still the same device: dropping the
        // address would orphan routing and leak an address from the pool.
        let path = temp_db("reset");
        let mut s = RedbJoinStore::open(&path).unwrap();
        let before = s.assign_dev_addr(&[0x33; 8]).unwrap().dev_addr;
        s.admit_join(&[0x33; 8], 7).unwrap();
        s.reset_dev_nonce(&[0x33; 8]).unwrap();
        let after = s.assign_dev_addr(&[0x33; 8]).unwrap();
        assert_eq!(after.dev_addr, before);
        assert_eq!(after.previous, Some(before));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn records_written_before_the_allocator_existed_get_an_address() {
        // Forward-compat: an existing deployment's join_state rows have no dev_addr
        // field. They must load (serde default) and allocate on next join, not fail.
        let path = temp_db("migrate");
        let mut s = RedbJoinStore::open(&path).unwrap();
        let eui = [0x44u8; 8];
        // Write a legacy-shaped record: no dev_addr key at all.
        let legacy = r#"{"last_dev_nonce":5,"next_join_nonce":3,"seen":true,"recent_nonces":[5]}"#;
        let w = s.db.begin_write().unwrap();
        {
            let mut t = w.open_table(JOIN_STATE).unwrap();
            t.insert(key(&eui).as_str(), legacy).unwrap();
        }
        w.commit().unwrap();

        let a = s.assign_dev_addr(&eui).unwrap();
        assert_eq!(a.previous, None, "legacy record had no address");
        assert_eq!(a.dev_addr, DEV_ADDR_BASE);
        // ...and the pre-existing DevNonce state must be preserved, not clobbered.
        assert_eq!(s.last_dev_nonce(&eui), Some(5));
        let _ = std::fs::remove_file(&path);
    }
}
