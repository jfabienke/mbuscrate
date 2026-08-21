//! Durable join state for a LoRaWAN network side — the `std` half of `lorawan`.
//!
//! This lives in `mbus-rs` rather than `mbus-core` for the reason the core's own docs
//! give: *a dependency that needs `std` belongs in `mbus-rs`*. A join store is storage,
//! its errors are dynamic strings, and its in-memory implementation is a `BTreeMap` —
//! none of which belong in a crate whose portability rests on having almost nothing to
//! port. The *rules* it enforces (`admit_dev_nonce`, `admit_dev_nonce_windowed`,
//! `DevNoncePolicy`) are pure and stay in the core.
//!
//! Re-exported from `mbus_rs::lorawan`, so the path callers use is unchanged.

use std::collections::BTreeMap;

use mbus_core::lorawan::{
    admit_dev_nonce, admit_dev_nonce_windowed, DevNoncePolicy, DevNonceVerdict,
};

/// Outcome of admitting a whole join through a [`JoinStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinAdmission {
    /// Accepted: the DevNonce was recorded and this JoinNonce reserved, both durably.
    Admitted { join_nonce: u32 },
    /// Rejected as a DevNonce replay; nothing was changed.
    Replay { last: u16, seen: u16 },
}

/// Error from a [`JoinStore`] backend.
///
/// Derived, unlike most of the LoRaWAN code: it was hand-written only to keep `thiserror`
/// out of a portability-critical core, and that reason stopped applying the moment this
/// moved to `mbus-rs`, which already uses `thiserror` for [`crate::error::MBusError`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("join store: {0}")]
pub struct JoinStoreError(pub String);

/// Durable per-device join state — the persistence a 1.0.4 network side requires.
///
/// Implemented by the gateway (redb-backed in production, in memory for tests). The
/// single `admit_join` operation must be atomic and durable *before it returns*, so
/// the caller can rely on "recorded" being true before it transmits a JoinAccept —
/// the durable-before-live ordering that closes both replay windows.
pub trait JoinStore {
    /// Atomically, for `dev_eui`: apply [`admit_dev_nonce`]; if Fresh, record the
    /// DevNonce and reserve the next (strictly increasing) JoinNonce, both durable
    /// on return, and report `Admitted`; if Replay, change nothing and report it.
    fn admit_join(
        &mut self,
        dev_eui: &[u8; 8],
        dev_nonce: u16,
    ) -> Result<JoinAdmission, JoinStoreError>;

    /// Highest DevNonce recorded for `dev_eui`, or `None` if never seen.
    fn last_dev_nonce(&self, dev_eui: &[u8; 8]) -> Option<u16>;

    /// Clear a device's DevNonce high-water for a legitimate re-provision. Without
    /// this, a factory-reset device (DevNonce back to 0) is correctly but
    /// permanently rejected as a replay.
    fn reset_dev_nonce(&mut self, dev_eui: &[u8; 8]) -> Result<(), JoinStoreError>;

    /// Return the DevAddr for `dev_eui`, allocating one durably on first use.
    ///
    /// **A device keeps its address across rejoins.** The same DevEUI always gets the
    /// same DevAddr, so an autonomous rejoin does not orphan uplink routing or make an
    /// operator chase a new address; [`DevAddrAssignment::previous`] reports what it
    /// held before (`None` on a first-ever allocation), which is what distinguishes a
    /// first join from a re-join at the network side.
    ///
    /// Durability matters as much as it does for the JoinNonce: an allocator that lives
    /// only in memory restarts from its base on every process start, so two devices
    /// joining in separate runs are both handed the *same* address and their uplinks
    /// become indistinguishable. Implementations must commit before returning.
    fn assign_dev_addr(&mut self, dev_eui: &[u8; 8]) -> Result<DevAddrAssignment, JoinStoreError>;
}

/// Outcome of [`JoinStore::assign_dev_addr`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DevAddrAssignment {
    /// The address this device holds from now on.
    pub dev_addr: u32,
    /// What it held before this call: `None` if this is its first-ever allocation,
    /// otherwise the same value as `dev_addr` (addresses are stable per device).
    /// `Some` therefore means "this device has joined before" — a re-join.
    pub previous: Option<u32>,
}

impl DevAddrAssignment {
    /// True when the device already had an address, i.e. this join is a re-join.
    pub fn is_rejoin(&self) -> bool {
        self.previous.is_some()
    }
}

/// First DevAddr handed out by a store. `0x26xxxxxx` is the conventional
/// private/experimental range, matching the NetID the responder advertises.
pub const DEV_ADDR_BASE: u32 = 0x2600_0001;

/// In-memory [`JoinStore`] for tests and non-persistent bench use.
///
/// Correct while the process lives; it does not survive a restart, which is exactly
/// the gap the redb-backed store closes — so production must not use this.
#[derive(Debug, Default)]
pub struct InMemoryJoinStore {
    last_dev_nonce: BTreeMap<[u8; 8], u16>,
    next_join_nonce: BTreeMap<[u8; 8], u32>,
    /// Window of recently-accepted DevNonces per device, for the 1.0.2 policy.
    recent: BTreeMap<[u8; 8], Vec<u16>>,
    /// Stable per-device DevAddr assignments, and the next address to hand out.
    dev_addrs: BTreeMap<[u8; 8], u32>,
    next_dev_addr: Option<u32>,
    policy: DevNoncePolicy,
}

impl InMemoryJoinStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// With an explicit anti-replay policy (the default is
    /// [`DevNoncePolicy::RandomWindow`], correct for the 1.0.2 fleet).
    pub fn with_policy(policy: DevNoncePolicy) -> Self {
        Self {
            policy,
            ..Self::default()
        }
    }
}

impl JoinStore for InMemoryJoinStore {
    fn admit_join(
        &mut self,
        dev_eui: &[u8; 8],
        dev_nonce: u16,
    ) -> Result<JoinAdmission, JoinStoreError> {
        let last_hi = self.last_dev_nonce.get(dev_eui).copied();
        let verdict = match self.policy {
            DevNoncePolicy::Counter => admit_dev_nonce(last_hi, dev_nonce),
            DevNoncePolicy::RandomWindow { .. } => {
                let recent = self.recent.get(dev_eui).map(Vec::as_slice).unwrap_or(&[]);
                admit_dev_nonce_windowed(recent, last_hi, dev_nonce)
            }
        };
        if let DevNonceVerdict::Replay { last, seen } = verdict {
            return Ok(JoinAdmission::Replay { last, seen });
        }
        self.last_dev_nonce.insert(*dev_eui, dev_nonce);
        if let DevNoncePolicy::RandomWindow { keep } = self.policy {
            let w = self.recent.entry(*dev_eui).or_default();
            w.push(dev_nonce);
            if w.len() > keep {
                let excess = w.len() - keep;
                w.drain(0..excess);
            }
        }
        let jn = self.next_join_nonce.entry(*dev_eui).or_insert(1);
        let join_nonce = *jn;
        *jn = join_nonce.wrapping_add(1) & 0x00FF_FFFF;
        Ok(JoinAdmission::Admitted { join_nonce })
    }

    fn last_dev_nonce(&self, dev_eui: &[u8; 8]) -> Option<u16> {
        self.last_dev_nonce.get(dev_eui).copied()
    }

    fn reset_dev_nonce(&mut self, dev_eui: &[u8; 8]) -> Result<(), JoinStoreError> {
        self.last_dev_nonce.remove(dev_eui);
        self.recent.remove(dev_eui);
        Ok(())
    }

    fn assign_dev_addr(&mut self, dev_eui: &[u8; 8]) -> Result<DevAddrAssignment, JoinStoreError> {
        if let Some(&existing) = self.dev_addrs.get(dev_eui) {
            // Stable across rejoins: same device, same address.
            return Ok(DevAddrAssignment {
                dev_addr: existing,
                previous: Some(existing),
            });
        }
        let next = self.next_dev_addr.unwrap_or(DEV_ADDR_BASE);
        self.dev_addrs.insert(*dev_eui, next);
        self.next_dev_addr = Some(next.wrapping_add(1));
        Ok(DevAddrAssignment {
            dev_addr: next,
            previous: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_admits_advances_and_rejects_replays() {
        let mut s = InMemoryJoinStore::new();
        let eui = [0x00, 0x04, 0xA3, 0x0B, 0x00, 0xFF, 0x00, 0x01];

        // First join: admitted, JoinNonce starts at 1.
        assert_eq!(
            s.admit_join(&eui, 0).unwrap(),
            JoinAdmission::Admitted { join_nonce: 1 }
        );
        // Next fresh DevNonce: admitted, JoinNonce advances — never repeats.
        assert_eq!(
            s.admit_join(&eui, 1).unwrap(),
            JoinAdmission::Admitted { join_nonce: 2 }
        );
        // Replayed DevNonce (equal): rejected, and nothing advanced.
        assert_eq!(
            s.admit_join(&eui, 1).unwrap(),
            JoinAdmission::Replay { last: 1, seen: 1 }
        );
        // A later fresh one still gets JoinNonce 3, proving the replay did not burn one.
        assert_eq!(
            s.admit_join(&eui, 2).unwrap(),
            JoinAdmission::Admitted { join_nonce: 3 }
        );
        assert_eq!(s.last_dev_nonce(&eui), Some(2));
    }

    #[test]
    fn reset_clears_the_window_so_a_used_nonce_can_recur() {
        // Under the default (1.0.2 windowed) policy, a *reused* nonce is the replay
        // to guard against — not a merely-lower one. reset() clears the window so a
        // re-provisioned device may legitimately draw the same value again.
        let mut s = InMemoryJoinStore::new();
        let eui = [1, 2, 3, 4, 5, 6, 7, 8];
        s.admit_join(&eui, 100).unwrap();
        // Replaying the exact nonce is rejected...
        assert!(matches!(
            s.admit_join(&eui, 100).unwrap(),
            JoinAdmission::Replay { .. }
        ));
        // ...until an explicit re-provision clears the remembered window.
        s.reset_dev_nonce(&eui).unwrap();
        assert!(matches!(
            s.admit_join(&eui, 100).unwrap(),
            JoinAdmission::Admitted { .. }
        ));
    }

    #[test]
    fn windowed_store_admits_a_non_monotonic_random_sequence() {
        // A realistic LMIC-style random draw with values rising and falling: every
        // distinct value must be admitted, and JoinNonce advances once per admit.
        let mut s = InMemoryJoinStore::new();
        let eui = [0xEEu8; 8];
        let seq = [40_000u16, 12_000, 55_000, 3, 12_001, 41_000];
        for (i, &n) in seq.iter().enumerate() {
            assert_eq!(
                s.admit_join(&eui, n).unwrap(),
                JoinAdmission::Admitted {
                    join_nonce: (i + 1) as u32
                },
                "fresh random DevNonce {n} must be admitted",
            );
        }
        // Replaying any earlier value is now a replay, JoinNonce not burned.
        assert!(matches!(
            s.admit_join(&eui, 12_000).unwrap(),
            JoinAdmission::Replay { seen: 12_000, .. }
        ));
        assert_eq!(
            s.admit_join(&eui, 99).unwrap(),
            JoinAdmission::Admitted { join_nonce: 7 }
        );
    }

    #[test]
    fn window_forgets_beyond_keep_so_a_very_old_nonce_may_recur() {
        // Bounded history: once a nonce falls out of the last `keep`, it is no longer
        // remembered — acceptable because meter join cadence is rare and the MIC
        // still binds every request. keep=2 makes the boundary easy to see.
        let mut s = InMemoryJoinStore::with_policy(DevNoncePolicy::RandomWindow { keep: 2 });
        let eui = [7u8; 8];
        s.admit_join(&eui, 10).unwrap(); // window [10]
        s.admit_join(&eui, 20).unwrap(); // window [10,20]
        s.admit_join(&eui, 30).unwrap(); // window [20,30] — 10 evicted
                                         // 10 is no longer remembered, so it is admitted again.
        assert!(matches!(
            s.admit_join(&eui, 10).unwrap(),
            JoinAdmission::Admitted { .. }
        ));
        // 30 is still in the window, so it is still a replay.
        assert!(matches!(
            s.admit_join(&eui, 30).unwrap(),
            JoinAdmission::Replay { .. }
        ));
    }

    #[test]
    fn counter_policy_still_enforces_strict_increase() {
        // Opt-in 1.0.4 hardening remains available and unchanged.
        let mut s = InMemoryJoinStore::with_policy(DevNoncePolicy::Counter);
        let eui = [3u8; 8];
        assert!(matches!(
            s.admit_join(&eui, 5).unwrap(),
            JoinAdmission::Admitted { .. }
        ));
        assert!(matches!(
            s.admit_join(&eui, 4).unwrap(),
            JoinAdmission::Replay { .. }
        ));
        assert!(matches!(
            s.admit_join(&eui, 6).unwrap(),
            JoinAdmission::Admitted { .. }
        ));
    }

    #[test]
    fn join_nonce_is_per_device() {
        let mut s = InMemoryJoinStore::new();
        let a = [0xAAu8; 8];
        let b = [0xBBu8; 8];
        // Each device gets its own monotonic sequence starting at 1.
        assert_eq!(
            s.admit_join(&a, 0).unwrap(),
            JoinAdmission::Admitted { join_nonce: 1 }
        );
        assert_eq!(
            s.admit_join(&b, 0).unwrap(),
            JoinAdmission::Admitted { join_nonce: 1 }
        );
        assert_eq!(
            s.admit_join(&a, 1).unwrap(),
            JoinAdmission::Admitted { join_nonce: 2 }
        );
    }
}
