//! LoRaWAN join-control protocol — the shared message schema between the optical
//! `device-config` app and the RF gateway (`metermon`) for coordinating an OTAA join.
//!
//! This crate is a **standalone, dependency-light** shared type: both apps add it as a
//! path dependency so the wire schema has a single source of truth.
//!
//! ```toml
//! # device-config (in core/Cargo.toml) and metermon-rs both add:
//! lorawan-join-control = { path = "../../lorawan-join-control" }
//! ```
//!
//! Transport-agnostic serde types. In deployment they ride the gateway's existing
//! MQTT (topic layout in [`topics`]); nothing here depends on MQTT, so the same JSON
//! works over any transport.
//!
//! ## Why this exists
//! An OTAA join here is a two-app dance. `device-config` drives the meter optically
//! (SET OTAA `0x24`, Join Request `0x06`, read `CheckJoinAccept 0x07`); the gateway's
//! single-channel responder hears the JoinRequest, MIC-checks it, and transmits the
//! JoinAccept. Neither side alone can confirm the whole join — the gateway knows it
//! *sent* an accept, only the meter knows it *received* one — so they exchange these
//! messages and agree on one [`JoinOutcome`].
//!
//! ## Sequence
//! 1. device-config → gateway: [`ArmRequest`]   — park the responder on channel + SF
//! 2. gateway → device-config: [`ArmReply`]     — armed + what it's listening on
//! 3. device-config triggers the optical join, emitting a [`FiredNotice`] per attempt
//! 4. gateway → device-config: [`JoinStatus`]   — heard? mic_ok? assigned DevAddr?
//! 5. device-config reads the meter's `0x07` and sends a [`VerifyRequest`]
//! 6. gateway → device-config: [`VerifyReply`] carrying the final [`JoinOutcome`]
//!
//! ## Autonomous rejoin (out-of-band, no orchestration)
//! A device provisioned **OTAA + ADR-on** self-heals: when downlink contact is lost it
//! runs the LoRaWAN ADR-ACK back-off (widen DR/channels/power) and ultimately emits a
//! fresh JoinRequest — with *no optical head present*, no [`ArmRequest`], no
//! [`FiredNotice`], and no way to read the meter's `0x07`. Only the gateway witnesses it.
//! The gateway answers from its **standing provisioning** (known DevEUI→AppKey, persistent
//! — not the 120 s arm window) and reports the event as a single [`RejoinObserved`] on the
//! [`topics::rejoin`] topic. Because no optical head confirms it, the resulting
//! [`JoinOutcome::RejoinedGatewaySide`] is **gateway-attested only** — it is *not*
//! [`JoinOutcome::is_confirmed`].
//!
//! Corollary for the responder's downlink policy: the heal is triggered by downlink
//! starvation, so the gateway should answer `ADRACKReq` with a MAC/empty downlink whenever
//! it *hears* the device — which suppresses needless rejoins exactly when the link is fine,
//! and (since a downlink is only possible in the RX window of an uplink it heard) leaves the
//! heal to engage on its own when the device is genuinely out of contact. Self-correcting;
//! no scheduling required.
//!
//! ## Key custody
//! Credentials (the AppKey) are **never** carried in this protocol. They are
//! provisioned out-of-band and held by the gateway; every message here references a
//! device only by its DevEUI.

use serde::{Deserialize, Serialize};

/// A LoRaWAN DevEUI as big-endian hex (16 chars), e.g. `"04B648FC80257775"`.
pub type DevEuiHex = String;

/// device-config → gateway: arm the single-channel responder for a device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArmRequest {
    pub dev_eui: DevEuiHex,
    /// Join channel the device transmits on, in Hz (e.g. `868_500_000`).
    pub channel_hz: u32,
    /// Spreading factor of the device's JoinRequest (7..=12). SF12 = DR0 for EU868.
    pub sf: u8,
}

/// gateway → device-config: responder armed and listening.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArmReply {
    pub dev_eui: DevEuiHex,
    pub armed: bool,
    pub listening_hz: u32,
    pub listening_sf: u8,
    /// Whether the gateway holds provisioned credentials for this DevEUI. `false`
    /// means arm will hear the JoinRequest but cannot MIC-check or answer it.
    pub creds_present: bool,
}

/// device-config → gateway: an optical Join Request was just triggered (for
/// timestamp correlation against the responder's receive log).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FiredNotice {
    pub dev_eui: DevEuiHex,
    pub ts_unix: u64,
    /// 1-based attempt counter within the current arm window.
    pub fire_seq: u32,
}

/// gateway → device-config: what the responder has observed for this device so far.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JoinStatus {
    pub dev_eui: DevEuiHex,
    /// A JoinRequest was received on the armed channel/SF.
    pub heard: bool,
    /// MIC verification result, once a JoinRequest was heard.
    pub mic_ok: Option<bool>,
    /// DevAddr the gateway assigned in the JoinAccept it sent, if any.
    pub assigned_dev_addr: Option<u32>,
    /// When the JoinAccept was transmitted (unix seconds).
    pub accept_ts_unix: Option<u64>,
    /// Last uplink RSSI (dBm) / SNR (dB) — link-budget diagnostics.
    pub rssi_dbm: Option<i16>,
    pub snr_db: Option<f32>,
}

/// device-config → gateway: the meter's own view, read from `CheckJoinAccept (0x07)`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyRequest {
    pub dev_eui: DevEuiHex,
    /// DevAddr the meter reports it received; `0` = none (not joined device-side).
    pub device_dev_addr: u32,
    pub device_net_id: u32,
}

/// gateway → device-config: the agreed end-to-end result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifyReply {
    pub dev_eui: DevEuiHex,
    pub outcome: JoinOutcome,
}

/// gateway → device-config / backend: an **unsolicited** rejoin was observed for a
/// provisioned device — no arm, no fire, no optical head present (§ *Autonomous rejoin*).
///
/// This is the self-heal path: the gateway answered a JoinRequest from its standing
/// provisioning and (re)confirmed the device's DevAddr. There is no device-side `0x07`
/// readback to reconcile against, so this event stands on the gateway's attestation alone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RejoinObserved {
    pub dev_eui: DevEuiHex,
    /// DevAddr the device held *before* this event, from the gateway's durable store.
    /// `None` only on the device's very first join (never seen before). The gateway
    /// assigns addresses **stably per device**, so on a rejoin this equals
    /// [`new_dev_addr`](Self::new_dev_addr) — i.e. `prev_dev_addr.is_some()` means "has
    /// joined before", **not** "the address changed". (A value is never fabricated from a
    /// non-durable counter; it comes from the gateway's persistent per-DevEUI store.)
    pub prev_dev_addr: Option<u32>,
    /// DevAddr in the JoinAccept the gateway just sent. **Stable per device**: a rejoin
    /// does not rotate the address, so on a rejoin this equals
    /// [`prev_dev_addr`](Self::prev_dev_addr). Stability avoids orphaning uplink routing
    /// when a device self-heals.
    pub new_dev_addr: u32,
    /// When the rejoin was observed (unix seconds).
    pub ts_unix: u64,
    /// Uplink RSSI (dBm) / SNR (dB) of the JoinRequest — link-budget diagnostics.
    pub rssi_dbm: Option<i16>,
    pub snr_db: Option<f32>,
}

/// The end-to-end result both sides agree on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum JoinOutcome {
    /// No JoinRequest was heard on the armed channel/SF (wrong channel/SF, or the
    /// device never transmitted).
    NotHeard,
    /// A JoinRequest was heard but its MIC failed — wrong AppKey or corruption.
    HeardMicFail,
    /// The gateway assigned a DevAddr and sent the JoinAccept, but the meter's `0x07`
    /// does not show it: the downlink did not reach the device (RX-window timing,
    /// link budget, or receiver frequency offset). Real gateway-side, unconfirmed
    /// device-side.
    JoinedGatewayOnly { assigned_dev_addr: u32 },
    /// The meter's `0x07` DevAddr matches the gateway's assignment — joined and
    /// confirmed on both ends.
    VerifiedBothSides { dev_addr: u32 },
    /// An **autonomous** (self-heal) rejoin the gateway answered from standing
    /// provisioning — see [`RejoinObserved`]. Gateway-attested only: no optical head was
    /// present to confirm it device-side, so it is joined-and-real but *not*
    /// [`is_confirmed`](JoinOutcome::is_confirmed). The out-of-band counterpart of
    /// [`JoinedGatewayOnly`](JoinOutcome::JoinedGatewayOnly), which arises inside an
    /// orchestrated attempt.
    RejoinedGatewaySide { dev_addr: u32 },
}

impl JoinOutcome {
    /// Reconcile the gateway's [`JoinStatus`] with the device's `0x07` DevAddr into a
    /// single verdict. This is the core of the verification handshake.
    pub fn reconcile(status: &JoinStatus, device_dev_addr: u32) -> Self {
        match (status.heard, status.mic_ok, status.assigned_dev_addr) {
            (false, _, _) => JoinOutcome::NotHeard,
            (true, Some(false), _) => JoinOutcome::HeardMicFail,
            // Heard + MIC ok + an accept was sent: did the device receive it?
            (true, _, Some(assigned)) if assigned != 0 && device_dev_addr == assigned => {
                JoinOutcome::VerifiedBothSides { dev_addr: assigned }
            }
            (true, _, Some(assigned)) if assigned != 0 => JoinOutcome::JoinedGatewayOnly {
                assigned_dev_addr: assigned,
            },
            // Heard, but no accept was ever sent (e.g. MIC unknown / no creds).
            (true, _, _) => JoinOutcome::HeardMicFail,
        }
    }

    /// Whether the join is confirmed end-to-end (the only fully-successful state).
    ///
    /// `RejoinedGatewaySide` is deliberately **not** confirmed: a self-heal has no optical
    /// witness, so it is real gateway-side but unverified device-side.
    pub fn is_confirmed(&self) -> bool {
        matches!(self, JoinOutcome::VerifiedBothSides { .. })
    }

    /// Whether the device is joined on the gateway but **not** confirmed device-side —
    /// the "real gateway-side, unverified" states: an orchestrated
    /// [`JoinedGatewayOnly`](JoinOutcome::JoinedGatewayOnly) or an out-of-band
    /// [`RejoinedGatewaySide`](JoinOutcome::RejoinedGatewaySide).
    pub fn is_joined_gateway_side(&self) -> bool {
        matches!(
            self,
            JoinOutcome::JoinedGatewayOnly { .. } | JoinOutcome::RejoinedGatewaySide { .. }
        )
    }
}

/// MQTT topic layout for the join-control protocol, namespaced per gateway id.
///
/// Rides the gateway's existing MQTT broker; the payload of each topic is the JSON of
/// the correspondingly-named type above.
pub mod topics {
    /// device-config → gateway.
    pub fn arm(gateway_id: &str) -> String {
        format!("join/{gateway_id}/arm")
    }
    pub fn fired(gateway_id: &str) -> String {
        format!("join/{gateway_id}/fired")
    }
    pub fn verify(gateway_id: &str) -> String {
        format!("join/{gateway_id}/verify")
    }
    /// gateway → device-config.
    pub fn arm_reply(gateway_id: &str) -> String {
        format!("join/{gateway_id}/arm_reply")
    }
    pub fn status(gateway_id: &str, dev_eui: &str) -> String {
        format!("join/{gateway_id}/status/{dev_eui}")
    }
    pub fn verify_reply(gateway_id: &str) -> String {
        format!("join/{gateway_id}/verify_reply")
    }
    /// gateway → device-config / backend: an unsolicited [`RejoinObserved`](super::RejoinObserved)
    /// (out-of-band self-heal; no arm/fire/verify context).
    pub fn rejoin(gateway_id: &str) -> String {
        format!("join/{gateway_id}/rejoin")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(heard: bool, mic_ok: Option<bool>, assigned: Option<u32>) -> JoinStatus {
        JoinStatus {
            dev_eui: "04B648FC80257775".into(),
            heard,
            mic_ok,
            assigned_dev_addr: assigned,
            accept_ts_unix: Some(1_787_140_769),
            rssi_dbm: Some(-106),
            snr_db: Some(5.0),
        }
    }

    #[test]
    fn reconcile_covers_every_outcome() {
        // Nothing heard.
        assert_eq!(
            JoinOutcome::reconcile(&status(false, None, None), 0),
            JoinOutcome::NotHeard
        );
        // Heard but MIC failed.
        assert_eq!(
            JoinOutcome::reconcile(&status(true, Some(false), None), 0),
            JoinOutcome::HeardMicFail
        );
        // Our live case: gateway assigned 0x26000001, device 0x07 still 0 → gateway-only.
        assert_eq!(
            JoinOutcome::reconcile(&status(true, Some(true), Some(0x2600_0001)), 0),
            JoinOutcome::JoinedGatewayOnly {
                assigned_dev_addr: 0x2600_0001
            }
        );
        // Device confirms the same DevAddr → verified both sides.
        let both =
            JoinOutcome::reconcile(&status(true, Some(true), Some(0x2600_0001)), 0x2600_0001);
        assert_eq!(
            both,
            JoinOutcome::VerifiedBothSides {
                dev_addr: 0x2600_0001
            }
        );
        assert!(both.is_confirmed());
        // A mismatched device DevAddr is NOT a confirmation.
        assert!(
            !JoinOutcome::reconcile(&status(true, Some(true), Some(0x2600_0002)), 0x2600_0001)
                .is_confirmed()
        );
    }

    #[test]
    fn json_round_trips_and_tags_the_outcome() {
        let reply = VerifyReply {
            dev_eui: "04B648FC80257775".into(),
            outcome: JoinOutcome::JoinedGatewayOnly {
                assigned_dev_addr: 0x2600_0001,
            },
        };
        let json = serde_json::to_string(&reply).unwrap();
        assert!(json.contains("\"outcome\":\"joined_gateway_only\""));
        assert!(json.contains("\"assigned_dev_addr\":637534209")); // 0x26000001
        let back: VerifyReply = serde_json::from_str(&json).unwrap();
        assert_eq!(reply, back);
    }

    #[test]
    fn topics_are_namespaced_per_gateway() {
        assert_eq!(topics::arm("gw-pi5-01"), "join/gw-pi5-01/arm");
        assert_eq!(
            topics::status("gw-pi5-01", "04B648FC80257775"),
            "join/gw-pi5-01/status/04B648FC80257775"
        );
        assert_eq!(topics::rejoin("gw-pi5-01"), "join/gw-pi5-01/rejoin");
    }

    #[test]
    fn rejoin_gateway_side_is_joined_but_not_confirmed() {
        let r = JoinOutcome::RejoinedGatewaySide {
            dev_addr: 0x2600_0007,
        };
        // Real gateway-side, but no optical witness → not end-to-end confirmed.
        assert!(!r.is_confirmed());
        assert!(r.is_joined_gateway_side());
        // The orchestrated gateway-only state groups with it; the confirmed/failed ones don't.
        assert!(JoinOutcome::JoinedGatewayOnly {
            assigned_dev_addr: 0x2600_0007
        }
        .is_joined_gateway_side());
        assert!(!JoinOutcome::VerifiedBothSides {
            dev_addr: 0x2600_0007
        }
        .is_joined_gateway_side());
        assert!(!JoinOutcome::NotHeard.is_joined_gateway_side());
    }

    #[test]
    fn rejoin_observed_round_trips_first_join() {
        // First join: never seen before, so prev_dev_addr is None.
        let ev = RejoinObserved {
            dev_eui: "04B648FC80257775".into(),
            prev_dev_addr: None,
            new_dev_addr: 0x2600_0007,
            ts_unix: 1_787_140_769,
            rssi_dbm: Some(-98),
            snr_db: Some(3.5),
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"prev_dev_addr\":null"));
        assert!(json.contains("\"new_dev_addr\":637534215")); // 0x26000007
        let back: RejoinObserved = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, back);
    }

    #[test]
    fn rejoin_keeps_stable_dev_addr() {
        // Stable-per-device addressing: on a re-join prev == new. "Has joined before" is
        // prev_dev_addr.is_some(), NOT prev != new (which stays false — no rotation).
        let ev = RejoinObserved {
            dev_eui: "04B648FC80257775".into(),
            prev_dev_addr: Some(0x2600_0007),
            new_dev_addr: 0x2600_0007,
            ts_unix: 1_787_227_200,
            rssi_dbm: Some(-101),
            snr_db: Some(1.0),
        };
        assert!(ev.prev_dev_addr.is_some(), "device has joined before");
        assert_eq!(ev.prev_dev_addr, Some(ev.new_dev_addr), "address is stable");
        let back: RejoinObserved =
            serde_json::from_str(&serde_json::to_string(&ev).unwrap()).unwrap();
        assert_eq!(ev, back);
    }

    #[test]
    fn rejoined_gateway_side_tags_snake_case() {
        let json = serde_json::to_string(&JoinOutcome::RejoinedGatewaySide {
            dev_addr: 0x2600_0007,
        })
        .unwrap();
        assert!(json.contains("\"outcome\":\"rejoined_gateway_side\""));
    }
}
