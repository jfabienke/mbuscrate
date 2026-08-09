//! LoRaWAN 1.0.x join and data-frame cryptography.
//!
//! Enough of a network server to complete an OTAA join and read the uplinks that
//! follow — deliberately not a LoRaWAN network server. It has no DevNonce replay
//! store, no frame-counter policy, no MAC-command handling and no duty-cycle
//! accounting; those are the long tail that separates "answers a join" from "runs
//! a network", and the gateway names the difference rather than blurring it.
//!
//! Targets **1.0.2**, the version confirmed on the Zenner hardware. 1.1 changes
//! the join MIC inputs and adds a NwkKey, so these routines do not carry over.
//!
//! Everything here is pure: bytes in, bytes out, no radio and no clock. The
//! timing-critical half (receive windows, transmission) belongs to the gateway,
//! and key ownership belongs to the Device Manager.
//!
//! ## Endianness
//!
//! LoRaWAN puts multi-byte fields on air **little-endian** — EUIs, DevAddr, DevNonce,
//! FCnt. The `_le` suffixes below are a reminder that these are wire-order bytes,
//! not display order: an EUI shown as `70:B3:D5:...` is transmitted reversed.

#![cfg(feature = "crypto")]

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;
use cmac::{Cmac, Mac};

/// Errors from parsing or authenticating a LoRaWAN frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoRaWanError {
    /// Frame is shorter than its own structure requires.
    TooShort { needed: usize, actual: usize },
    /// MHDR message type is not the one this parser expects.
    WrongMessageType { expected: u8, actual: u8 },
    /// The computed MIC does not match the one on air.
    MicMismatch,
    /// A field was outside its permitted range.
    InvalidField(&'static str),
}

impl std::fmt::Display for LoRaWanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { needed, actual } => {
                write!(f, "frame too short: needed {needed}, got {actual}")
            }
            Self::WrongMessageType { expected, actual } => {
                write!(f, "wrong message type: expected {expected}, got {actual}")
            }
            Self::MicMismatch => write!(f, "MIC mismatch"),
            Self::InvalidField(name) => write!(f, "invalid field: {name}"),
        }
    }
}

impl std::error::Error for LoRaWanError {}

/// MHDR message types (bits 7:5).
pub const MTYPE_JOIN_REQUEST: u8 = 0x00;
pub const MTYPE_JOIN_ACCEPT: u8 = 0x01;
pub const MTYPE_UNCONFIRMED_UP: u8 = 0x02;
pub const MTYPE_UNCONFIRMED_DOWN: u8 = 0x03;
pub const MTYPE_CONFIRMED_UP: u8 = 0x04;
pub const MTYPE_CONFIRMED_DOWN: u8 = 0x05;

fn mtype(mhdr: u8) -> u8 {
    mhdr >> 5
}

fn aes_encrypt_block(key: &[u8; 16], block: &mut [u8; 16]) {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    cipher.encrypt_block(GenericArray::from_mut_slice(block));
}

fn aes_decrypt_block(key: &[u8; 16], block: &mut [u8; 16]) {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    cipher.decrypt_block(GenericArray::from_mut_slice(block));
}

fn cmac16(key: &[u8; 16], data: &[u8]) -> [u8; 16] {
    let mut mac = <Cmac<Aes128> as Mac>::new_from_slice(key).expect("128-bit key");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// First four bytes of the CMAC — every LoRaWAN MIC.
fn mic4(key: &[u8; 16], data: &[u8]) -> [u8; 4] {
    let full = cmac16(key, data);
    [full[0], full[1], full[2], full[3]]
}

/// Session keys derived at join, held by both sides and never transmitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionKeys {
    pub nwk_skey: [u8; 16],
    pub app_skey: [u8; 16],
    pub dev_addr: u32,
}

/// A parsed JoinRequest (23 bytes on air).
///
/// EUIs are kept in **wire order** (little-endian). Use [`JoinRequest::dev_eui_display`]
/// for the conventional big-endian rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinRequest {
    pub join_eui_le: [u8; 8],
    pub dev_eui_le: [u8; 8],
    pub dev_nonce: u16,
    mic: [u8; 4],
    /// MHDR..DevNonce — the exact bytes the MIC covers.
    signed: Vec<u8>,
}

impl JoinRequest {
    /// Parse without authenticating. Call [`JoinRequest::verify_mic`] before trusting
    /// any field: the EUIs are attacker-controlled until the MIC checks out.
    pub fn parse(frame: &[u8]) -> Result<Self, LoRaWanError> {
        if frame.len() < 23 {
            return Err(LoRaWanError::TooShort {
                needed: 23,
                actual: frame.len(),
            });
        }
        if mtype(frame[0]) != MTYPE_JOIN_REQUEST {
            return Err(LoRaWanError::WrongMessageType {
                expected: MTYPE_JOIN_REQUEST,
                actual: mtype(frame[0]),
            });
        }
        let mut join_eui_le = [0u8; 8];
        let mut dev_eui_le = [0u8; 8];
        join_eui_le.copy_from_slice(&frame[1..9]);
        dev_eui_le.copy_from_slice(&frame[9..17]);
        Ok(Self {
            join_eui_le,
            dev_eui_le,
            dev_nonce: u16::from_le_bytes([frame[17], frame[18]]),
            mic: [frame[19], frame[20], frame[21], frame[22]],
            signed: frame[..19].to_vec(),
        })
    }

    /// Whether the frame authenticates under `app_key`.
    ///
    /// This is also the device-identity check: a JoinRequest whose MIC verifies was
    /// produced by something holding the AppKey we hold for that DevEUI.
    pub fn verify_mic(&self, app_key: &[u8; 16]) -> bool {
        mic4(app_key, &self.signed) == self.mic
    }

    /// DevEUI in conventional display order (big-endian, colon-separated).
    pub fn dev_eui_display(&self) -> String {
        eui_display(&self.dev_eui_le)
    }

    /// JoinEUI in conventional display order.
    pub fn join_eui_display(&self) -> String {
        eui_display(&self.join_eui_le)
    }
}

/// Render a wire-order (little-endian) EUI big-endian, colon-separated.
pub fn eui_display(le: &[u8; 8]) -> String {
    le.iter()
        .rev()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Everything the network chooses when accepting a join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinAcceptParams {
    /// Network-chosen nonce, 24-bit. Must differ per join for the session keys to differ.
    pub app_nonce: u32,
    /// Network identifier, 24-bit. 0x000000 is the private/experimental range.
    pub net_id: u32,
    /// Address assigned to the device for this session.
    pub dev_addr: u32,
    /// RX1 data-rate offset and RX2 data rate, packed as the spec defines.
    pub dl_settings: u8,
    /// Delay in seconds before RX1 opens; 0 is interpreted as 1.
    pub rx_delay: u8,
}

impl Default for JoinAcceptParams {
    fn default() -> Self {
        Self {
            app_nonce: 0,
            net_id: 0,
            dev_addr: 0,
            dl_settings: 0,
            // JOIN_ACCEPT_DELAY1 is fixed at 5 s and is NOT this field: RxDelay
            // governs the delay after *data* uplinks. 1 s is the EU868 default.
            rx_delay: 1,
        }
    }
}

/// Build a JoinAccept, encrypted and ready to transmit (17 bytes, no CFList).
///
/// The encryption is inverted by LoRaWAN's own convention: the network applies the
/// AES **decrypt** operation, so a device holding only an encrypt primitive can
/// recover it. Getting this backwards produces a frame the device silently ignores.
pub fn build_join_accept(app_key: &[u8; 16], p: &JoinAcceptParams) -> Vec<u8> {
    let mhdr = MTYPE_JOIN_ACCEPT << 5;
    let mut plain = Vec::with_capacity(16);
    plain.extend_from_slice(&p.app_nonce.to_le_bytes()[..3]);
    plain.extend_from_slice(&p.net_id.to_le_bytes()[..3]);
    plain.extend_from_slice(&p.dev_addr.to_le_bytes());
    plain.push(p.dl_settings);
    plain.push(p.rx_delay);

    // MIC covers MHDR and the *plaintext* body.
    let mut signed = Vec::with_capacity(1 + plain.len());
    signed.push(mhdr);
    signed.extend_from_slice(&plain);
    let mic = mic4(app_key, &signed);
    plain.extend_from_slice(&mic); // exactly one 16-byte block

    let mut block = [0u8; 16];
    block.copy_from_slice(&plain);
    aes_decrypt_block(app_key, &mut block);

    let mut out = Vec::with_capacity(17);
    out.push(mhdr);
    out.extend_from_slice(&block);
    out
}

/// Derive the session keys for a join, per LoRaWAN 1.0.x.
///
/// Both sides run this independently from values they already hold, which is why no
/// session key is ever transmitted.
pub fn derive_session_keys(
    app_key: &[u8; 16],
    p: &JoinAcceptParams,
    dev_nonce: u16,
) -> SessionKeys {
    let derive = |first: u8| -> [u8; 16] {
        let mut block = [0u8; 16];
        block[0] = first;
        block[1..4].copy_from_slice(&p.app_nonce.to_le_bytes()[..3]);
        block[4..7].copy_from_slice(&p.net_id.to_le_bytes()[..3]);
        block[7..9].copy_from_slice(&dev_nonce.to_le_bytes());
        // bytes 9..16 stay zero
        aes_encrypt_block(app_key, &mut block);
        block
    };
    SessionKeys {
        nwk_skey: derive(0x01),
        app_skey: derive(0x02),
        dev_addr: p.dev_addr,
    }
}

// ============================ 1.0.4 anti-replay ============================
//
// LoRaWAN 1.0.4 makes DevNonce a device-side monotonic counter and JoinNonce a
// network-side one, each checked for strict increase to reject replays. Enforcing
// that needs durable per-device state; the *rules* live here as pure logic, the
// *storage* is a gateway concern behind [`JoinStore`]. See
// docs/design/lorawan-join-persistence.md.

/// Result of checking a JoinRequest's DevNonce against the highest one accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevNonceVerdict {
    /// First-ever, or strictly greater than the last accepted — accept.
    Fresh,
    /// Not strictly greater than the last accepted — a replay, reject.
    Replay { last: u16, seen: u16 },
}

/// The 1.0.4 DevNonce freshness rule, in isolation: strictly greater, or first-seen.
pub fn admit_dev_nonce(last: Option<u16>, seen: u16) -> DevNonceVerdict {
    match last {
        None => DevNonceVerdict::Fresh,
        Some(l) if seen > l => DevNonceVerdict::Fresh,
        Some(l) => DevNonceVerdict::Replay { last: l, seen },
    }
}

/// Outcome of admitting a whole join through a [`JoinStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinAdmission {
    /// Accepted: the DevNonce was recorded and this JoinNonce reserved, both durably.
    Admitted { join_nonce: u32 },
    /// Rejected as a DevNonce replay; nothing was changed.
    Replay { last: u16, seen: u16 },
}

/// Error from a [`JoinStore`] backend.
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
}

/// In-memory [`JoinStore`] for tests and non-persistent bench use.
///
/// Correct while the process lives; it does not survive a restart, which is exactly
/// the gap the redb-backed store closes — so production must not use this.
#[derive(Debug, Default)]
pub struct InMemoryJoinStore {
    last_dev_nonce: std::collections::HashMap<[u8; 8], u16>,
    next_join_nonce: std::collections::HashMap<[u8; 8], u32>,
}

impl InMemoryJoinStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl JoinStore for InMemoryJoinStore {
    fn admit_join(
        &mut self,
        dev_eui: &[u8; 8],
        dev_nonce: u16,
    ) -> Result<JoinAdmission, JoinStoreError> {
        if let DevNonceVerdict::Replay { last, seen } =
            admit_dev_nonce(self.last_dev_nonce.get(dev_eui).copied(), dev_nonce)
        {
            return Ok(JoinAdmission::Replay { last, seen });
        }
        self.last_dev_nonce.insert(*dev_eui, dev_nonce);
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
        Ok(())
    }
}

/// A parsed data frame (uplink or downlink), before authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataFrame {
    pub mhdr: u8,
    pub dev_addr: u32,
    pub fctrl: u8,
    pub fcnt: u16,
    pub fopts: Vec<u8>,
    pub fport: Option<u8>,
    /// Still encrypted; see [`DataFrame::decrypt_payload`].
    pub frm_payload: Vec<u8>,
    mic: [u8; 4],
    signed: Vec<u8>,
}

impl DataFrame {
    pub fn parse(frame: &[u8]) -> Result<Self, LoRaWanError> {
        // MHDR(1) DevAddr(4) FCtrl(1) FCnt(2) [FOpts] [FPort] [FRMPayload] MIC(4)
        if frame.len() < 12 {
            return Err(LoRaWanError::TooShort {
                needed: 12,
                actual: frame.len(),
            });
        }
        let fctrl = frame[5];
        let fopts_len = (fctrl & 0x0F) as usize;
        let header_len = 8 + fopts_len;
        if frame.len() < header_len + 4 {
            return Err(LoRaWanError::TooShort {
                needed: header_len + 4,
                actual: frame.len(),
            });
        }
        let body_end = frame.len() - 4;
        let (fport, frm_payload) = if body_end > header_len {
            (
                Some(frame[header_len]),
                frame[header_len + 1..body_end].to_vec(),
            )
        } else {
            (None, Vec::new())
        };
        Ok(Self {
            mhdr: frame[0],
            dev_addr: u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]]),
            fctrl,
            fcnt: u16::from_le_bytes([frame[6], frame[7]]),
            fopts: frame[8..header_len].to_vec(),
            fport,
            frm_payload,
            mic: [
                frame[body_end],
                frame[body_end + 1],
                frame[body_end + 2],
                frame[body_end + 3],
            ],
            signed: frame[..body_end].to_vec(),
        })
    }

    /// True for uplinks (device to network).
    pub fn is_uplink(&self) -> bool {
        matches!(mtype(self.mhdr), MTYPE_UNCONFIRMED_UP | MTYPE_CONFIRMED_UP)
    }

    /// Verify the frame MIC with the network session key.
    ///
    /// `fcnt_full` is the 32-bit counter: the frame carries only its low 16 bits, so
    /// the caller supplies the upper half it is tracking. Getting that wrong fails the
    /// MIC in a way indistinguishable from a forgery, which is why it is explicit.
    pub fn verify_mic(&self, nwk_skey: &[u8; 16], fcnt_full: u32) -> bool {
        let mut b0 = [0u8; 16];
        b0[0] = 0x49;
        b0[5] = if self.is_uplink() { 0 } else { 1 };
        b0[6..10].copy_from_slice(&self.dev_addr.to_le_bytes());
        b0[10..14].copy_from_slice(&fcnt_full.to_le_bytes());
        b0[15] = self.signed.len() as u8;

        let mut buf = Vec::with_capacity(16 + self.signed.len());
        buf.extend_from_slice(&b0);
        buf.extend_from_slice(&self.signed);
        mic4(nwk_skey, &buf) == self.mic
    }

    /// Decrypt FRMPayload. Uses AppSKey for application ports, NwkSKey for port 0
    /// (MAC commands) — the spec's split, and using the wrong one yields plausible
    /// garbage rather than an error.
    pub fn decrypt_payload(
        &self,
        nwk_skey: &[u8; 16],
        app_skey: &[u8; 16],
        fcnt_full: u32,
    ) -> Vec<u8> {
        let key = if self.fport == Some(0) {
            nwk_skey
        } else {
            app_skey
        };
        let dir = if self.is_uplink() { 0u8 } else { 1u8 };
        let mut out = Vec::with_capacity(self.frm_payload.len());
        for (i, chunk) in self.frm_payload.chunks(16).enumerate() {
            let mut a = [0u8; 16];
            a[0] = 0x01;
            a[5] = dir;
            a[6..10].copy_from_slice(&self.dev_addr.to_le_bytes());
            a[10..14].copy_from_slice(&fcnt_full.to_le_bytes());
            a[15] = (i + 1) as u8;
            aes_encrypt_block(key, &mut a);
            for (b, s) in chunk.iter().zip(a.iter()) {
                out.push(b ^ s);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP_KEY: [u8; 16] = [
        0x2B, 0x7E, 0x15, 0x16, 0x28, 0xAE, 0xD2, 0xA6, 0xAB, 0xF7, 0x15, 0x88, 0x09, 0xCF, 0x4F,
        0x3C,
    ];

    fn make_join_request(app_key: &[u8; 16], dev_nonce: u16) -> Vec<u8> {
        let mut f = vec![MTYPE_JOIN_REQUEST << 5];
        f.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]); // JoinEUI LE
        f.extend_from_slice(&[0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18]); // DevEUI LE
        f.extend_from_slice(&dev_nonce.to_le_bytes());
        let mic = mic4(app_key, &f);
        f.extend_from_slice(&mic);
        f
    }

    #[test]
    fn join_request_round_trips_and_authenticates() {
        let raw = make_join_request(&APP_KEY, 0x1234);
        assert_eq!(raw.len(), 23);
        let jr = JoinRequest::parse(&raw).unwrap();
        assert_eq!(jr.dev_nonce, 0x1234);
        assert!(jr.verify_mic(&APP_KEY));
        // EUIs are transmitted reversed; display order is the human one.
        assert_eq!(jr.dev_eui_display(), "18:17:16:15:14:13:12:11");
        assert_eq!(jr.join_eui_display(), "08:07:06:05:04:03:02:01");
    }

    #[test]
    fn join_request_mic_rejects_a_wrong_key_and_any_tampering() {
        let raw = make_join_request(&APP_KEY, 0x1234);
        let mut wrong = APP_KEY;
        wrong[0] ^= 0x01;
        assert!(!JoinRequest::parse(&raw).unwrap().verify_mic(&wrong));

        // Flip one bit of the DevEUI: the MIC must fail, since it is the only thing
        // binding the claimed identity to the key.
        let mut tampered = raw.clone();
        tampered[9] ^= 0x01;
        assert!(!JoinRequest::parse(&tampered).unwrap().verify_mic(&APP_KEY));
    }

    #[test]
    fn join_request_rejects_short_frames_and_wrong_type() {
        assert!(matches!(
            JoinRequest::parse(&[0u8; 10]),
            Err(LoRaWanError::TooShort { .. })
        ));
        let mut raw = make_join_request(&APP_KEY, 1);
        raw[0] = MTYPE_UNCONFIRMED_UP << 5;
        assert!(matches!(
            JoinRequest::parse(&raw),
            Err(LoRaWanError::WrongMessageType { .. })
        ));
    }

    /// The device side of the handshake, to prove our JoinAccept is decodable by
    /// something that only runs AES *encrypt* — which is what the spec's inverted
    /// convention exists to allow.
    fn device_open_join_accept(app_key: &[u8; 16], frame: &[u8]) -> (JoinAcceptParams, bool) {
        let mut block = [0u8; 16];
        block.copy_from_slice(&frame[1..17]);
        aes_encrypt_block(app_key, &mut block);

        let params = JoinAcceptParams {
            app_nonce: u32::from_le_bytes([block[0], block[1], block[2], 0]),
            net_id: u32::from_le_bytes([block[3], block[4], block[5], 0]),
            dev_addr: u32::from_le_bytes([block[6], block[7], block[8], block[9]]),
            dl_settings: block[10],
            rx_delay: block[11],
        };
        let mut signed = vec![frame[0]];
        signed.extend_from_slice(&block[..12]);
        let mic_ok = mic4(app_key, &signed) == [block[12], block[13], block[14], block[15]];
        (params, mic_ok)
    }

    #[test]
    fn join_accept_is_recoverable_by_the_device_and_yields_the_same_keys() {
        let params = JoinAcceptParams {
            app_nonce: 0x00AB_CDEF,
            net_id: 0x0000_0013,
            dev_addr: 0x2601_1BDA,
            dl_settings: 0x00,
            rx_delay: 1,
        };
        let dev_nonce = 0x4E5Fu16;

        let frame = build_join_accept(&APP_KEY, &params);
        assert_eq!(frame.len(), 17, "no CFList: MHDR + one AES block");
        assert_eq!(frame[0] >> 5, MTYPE_JOIN_ACCEPT);

        let (recovered, mic_ok) = device_open_join_accept(&APP_KEY, &frame);
        assert!(mic_ok, "device must be able to authenticate the JoinAccept");
        assert_eq!(recovered.dev_addr, params.dev_addr);
        assert_eq!(recovered.app_nonce, params.app_nonce);
        assert_eq!(recovered.net_id, params.net_id);

        // Both sides derive identical session keys without exchanging them.
        let ours = derive_session_keys(&APP_KEY, &params, dev_nonce);
        let theirs = derive_session_keys(&APP_KEY, &recovered, dev_nonce);
        assert_eq!(ours, theirs);
        assert_ne!(
            ours.nwk_skey, ours.app_skey,
            "0x01/0x02 prefix must diverge"
        );
    }

    #[test]
    fn session_keys_change_with_every_nonce() {
        let base = JoinAcceptParams {
            app_nonce: 1,
            net_id: 0,
            dev_addr: 7,
            ..Default::default()
        };
        let a = derive_session_keys(&APP_KEY, &base, 100);
        let b = derive_session_keys(&APP_KEY, &base, 101); // different DevNonce
        let c = derive_session_keys(
            &APP_KEY,
            &JoinAcceptParams {
                app_nonce: 2,
                ..base
            },
            100, // different AppNonce
        );
        assert_ne!(a.nwk_skey, b.nwk_skey);
        assert_ne!(a.nwk_skey, c.nwk_skey);
    }

    /// Build an uplink the way a device does, then read it back as the network does.
    #[test]
    fn data_uplink_authenticates_and_decrypts() {
        let params = JoinAcceptParams {
            app_nonce: 0x0001_0203,
            net_id: 0,
            dev_addr: 0x0102_0304,
            ..Default::default()
        };
        let keys = derive_session_keys(&APP_KEY, &params, 42);
        let payload = b"\x0C\x13\x34\x12\x00\x00"; // a wM-Bus volume record
        let fcnt: u16 = 5;

        // Device side: encrypt, then MIC over the assembled frame.
        let mut frame = vec![MTYPE_UNCONFIRMED_UP << 5];
        frame.extend_from_slice(&params.dev_addr.to_le_bytes());
        frame.push(0x00); // FCtrl: no FOpts
        frame.extend_from_slice(&fcnt.to_le_bytes());
        frame.push(1); // FPort
        let stub = DataFrame {
            mhdr: MTYPE_UNCONFIRMED_UP << 5,
            dev_addr: params.dev_addr,
            fctrl: 0,
            fcnt,
            fopts: vec![],
            fport: Some(1),
            frm_payload: payload.to_vec(),
            mic: [0; 4],
            signed: vec![],
        };
        let encrypted = stub.decrypt_payload(&keys.nwk_skey, &keys.app_skey, fcnt as u32);
        frame.extend_from_slice(&encrypted);
        let mut b0 = [0u8; 16];
        b0[0] = 0x49;
        b0[6..10].copy_from_slice(&params.dev_addr.to_le_bytes());
        b0[10..14].copy_from_slice(&(fcnt as u32).to_le_bytes());
        b0[15] = frame.len() as u8;
        let mut signed = b0.to_vec();
        signed.extend_from_slice(&frame);
        frame.extend_from_slice(&mic4(&keys.nwk_skey, &signed));

        // Network side.
        let parsed = DataFrame::parse(&frame).unwrap();
        assert!(parsed.is_uplink());
        assert_eq!(parsed.fport, Some(1));
        assert!(parsed.verify_mic(&keys.nwk_skey, fcnt as u32));
        assert!(
            !parsed.verify_mic(&keys.nwk_skey, fcnt as u32 + 1),
            "a wrong frame counter must fail the MIC"
        );
        let plain = parsed.decrypt_payload(&keys.nwk_skey, &keys.app_skey, fcnt as u32);
        assert_eq!(plain, payload, "CTR-style stream is its own inverse");
    }

    #[test]
    fn data_frame_parses_fopts_and_empty_payloads() {
        // FCtrl low nibble = 3 FOpts bytes, and no FPort/FRMPayload at all.
        let mut frame = vec![MTYPE_UNCONFIRMED_UP << 5, 4, 3, 2, 1, 0x03, 9, 0];
        frame.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // FOpts
        frame.extend_from_slice(&[1, 2, 3, 4]); // MIC
        let f = DataFrame::parse(&frame).unwrap();
        assert_eq!(f.fopts, vec![0xAA, 0xBB, 0xCC]);
        assert_eq!(f.fport, None);
        assert!(f.frm_payload.is_empty());
        assert_eq!(f.fcnt, 9);
    }

    #[test]
    fn dev_nonce_rule_accepts_first_and_strictly_greater_only() {
        assert_eq!(admit_dev_nonce(None, 0), DevNonceVerdict::Fresh);
        assert_eq!(admit_dev_nonce(Some(5), 6), DevNonceVerdict::Fresh);
        assert_eq!(
            admit_dev_nonce(Some(5), 5),
            DevNonceVerdict::Replay { last: 5, seen: 5 }
        );
        assert_eq!(
            admit_dev_nonce(Some(5), 4),
            DevNonceVerdict::Replay { last: 5, seen: 4 }
        );
    }

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
    fn reset_allows_a_reprovisioned_device_to_rejoin_from_zero() {
        let mut s = InMemoryJoinStore::new();
        let eui = [1, 2, 3, 4, 5, 6, 7, 8];
        s.admit_join(&eui, 100).unwrap();
        // A device that reset to DevNonce 0 is correctly rejected as replay...
        assert!(matches!(
            s.admit_join(&eui, 0).unwrap(),
            JoinAdmission::Replay { .. }
        ));
        // ...until an explicit re-provision clears the high-water.
        s.reset_dev_nonce(&eui).unwrap();
        assert!(matches!(
            s.admit_join(&eui, 0).unwrap(),
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
