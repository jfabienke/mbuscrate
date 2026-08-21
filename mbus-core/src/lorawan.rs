//! LoRaWAN 1.0.x join and data-frame cryptography.
//!
//! Enough of a network server to complete an OTAA join and read the uplinks that
//! follow — deliberately not a LoRaWAN network server. DevNonce anti-replay *is*
//! handled (see [`DevNoncePolicy`] and the [`JoinStore`] trait, backed durably by
//! the gateway's redb store), but there is no frame-counter policy, no MAC-command
//! handling and no duty-cycle accounting; those are the long tail that separates
//! "answers a join" from "runs a network", and the gateway names the difference
//! rather than blurring it. See docs/design/lorawan-join-persistence.md.
//!
//! The join/session crypto is byte-identical across LoRaWAN 1.0.0–1.0.4 (1.0.4 only
//! renamed AppNonce→JoinNonce; the math is unchanged), which is what the Zenner
//! hardware runs. What *does* differ by version is the DevNonce anti-replay rule,
//! and that is [`DevNoncePolicy`]'s job. Only 1.1 breaks this module — it changes
//! the join MIC inputs and adds a NwkKey — so these routines stop at 1.0.x.
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

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;
use cmac::{Cmac, Mac};

// ============================ Capacities ============================
//
// Every bound here is a LoRaWAN protocol fact, not a tuning choice — which is the only
// reason fixed capacities are safe: a frame that does not fit was never representable on
// air. Where a combination *can* overflow (see [`build_data_down`]) the excess is a normal
// error, never a truncation and never a panic.

/// Largest PHYPayload LoRaWAN puts on air.
pub const PHY_PAYLOAD_MAX: usize = 255;

/// FOptsLen is a 4-bit field in FCtrl, so MAC commands in the FHDR cannot exceed this.
pub const FOPTS_MAX: usize = 15;

/// Largest FRMPayload: [`PHY_PAYLOAD_MAX`] less MHDR(1), the minimum FHDR(7), FPort(1)
/// and MIC(4). Regional limits are lower still (EU868 tops out at 222 on DR7); this is
/// the absolute structural bound, so nothing legal is refused.
pub const FRM_PAYLOAD_MAX: usize = PHY_PAYLOAD_MAX - 1 - 7 - 1 - 4;

/// A JoinAccept without a CFList is exactly this long: MHDR(1) + one AES block(16).
pub const JOIN_ACCEPT_LEN: usize = 17;

/// Bytes of a JoinRequest covered by the MIC: MHDR(1) JoinEUI(8) DevEUI(8) DevNonce(2).
const JOIN_REQUEST_SIGNED_LEN: usize = 19;

/// `XX:XX:...` for 8 bytes — 16 hex digits and 7 separators.
pub const EUI_DISPLAY_LEN: usize = 23;

/// A complete frame ready for the radio.
pub type Frame = heapless::Vec<u8, PHY_PAYLOAD_MAX>;
/// MAC commands carried in the FHDR.
pub type FOpts = heapless::Vec<u8, FOPTS_MAX>;
/// An application payload, encrypted or not.
pub type Payload = heapless::Vec<u8, FRM_PAYLOAD_MAX>;
/// An EUI in display form; see [`eui_display`].
pub type EuiString = heapless::String<EUI_DISPLAY_LEN>;

/// Errors from parsing or authenticating a LoRaWAN frame.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

impl core::fmt::Display for LoRaWanError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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

impl core::error::Error for LoRaWanError {}

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

/// First four bytes of the CMAC — every LoRaWAN MIC.
fn mic4(key: &[u8; 16], data: &[u8]) -> [u8; 4] {
    mic4_parts(key, &[data])
}

/// [`mic4`] over several slices in sequence, without joining them first.
///
/// LoRaWAN MICs are computed over `B0 || frame`, and the frame is itself several fields.
/// Concatenating them needed a heap buffer sized at run time; CMAC is a streaming
/// construction, so feeding the parts in order gives the identical result with no buffer
/// and no allocation.
fn mic4_parts(key: &[u8; 16], parts: &[&[u8]]) -> [u8; 4] {
    // `new` rather than `new_from_slice().expect(..)`: the key is a `&[u8; 16]`, so the
    // length is guaranteed by the type and the Result could never be Err. Taking the
    // infallible constructor removes a panic path that only existed because the fallible
    // API was the more obvious one to reach for.
    let mut mac = <Cmac<Aes128> as Mac>::new(GenericArray::from_slice(key));
    for p in parts {
        mac.update(p);
    }
    let full: [u8; 16] = mac.finalize().into_bytes().into();
    [full[0], full[1], full[2], full[3]]
}

/// Session keys derived at join, held by both sides and never transmitted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct JoinRequest {
    pub join_eui_le: [u8; 8],
    pub dev_eui_le: [u8; 8],
    pub dev_nonce: u16,
    mic: [u8; 4],
    /// MHDR..DevNonce — the exact bytes the MIC covers. A JoinRequest is fixed-length, so
    /// this is an array rather than a growable buffer: the size is known at compile time.
    signed: [u8; JOIN_REQUEST_SIGNED_LEN],
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
        let mut signed = [0u8; JOIN_REQUEST_SIGNED_LEN];
        join_eui_le.copy_from_slice(&frame[1..9]);
        dev_eui_le.copy_from_slice(&frame[9..17]);
        signed.copy_from_slice(&frame[..JOIN_REQUEST_SIGNED_LEN]);
        Ok(Self {
            join_eui_le,
            dev_eui_le,
            dev_nonce: u16::from_le_bytes([frame[17], frame[18]]),
            mic: [frame[19], frame[20], frame[21], frame[22]],
            signed,
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
    pub fn dev_eui_display(&self) -> EuiString {
        eui_display(&self.dev_eui_le)
    }

    /// JoinEUI in conventional display order.
    pub fn join_eui_display(&self) -> EuiString {
        eui_display(&self.join_eui_le)
    }
}

/// Render a wire-order (little-endian) EUI big-endian, colon-separated.
pub fn eui_display(le: &[u8; 8]) -> EuiString {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut s = EuiString::new();
    for (i, b) in le.iter().rev().enumerate() {
        // Exactly EUI_DISPLAY_LEN chars are pushed into a string of that capacity, so no
        // push can fail. The Results are discarded rather than unwrapped deliberately: an
        // `expect` here would be a panic path in a display helper, and the worst possible
        // consequence of a capacity slip is a short string, not a dead gateway.
        if i > 0 {
            let _ = s.push(':');
        }
        let _ = s.push(HEX[(b >> 4) as usize] as char);
        let _ = s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}

/// Everything the network chooses when accepting a join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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
/// Returns the frame as a fixed array: the body is AppNonce(3) NetID(3) DevAddr(4)
/// DLSettings(1) RxDelay(1) MIC(4) — exactly one 16-byte AES block, by construction, plus
/// MHDR. Nothing here is variable-length, so nothing needs to allocate.
pub fn build_join_accept(app_key: &[u8; 16], p: &JoinAcceptParams) -> [u8; JOIN_ACCEPT_LEN] {
    let mhdr = MTYPE_JOIN_ACCEPT << 5;

    // The 12-byte plaintext body, then the MIC appended to fill the block.
    let mut block = [0u8; 16];
    block[0..3].copy_from_slice(&p.app_nonce.to_le_bytes()[..3]);
    block[3..6].copy_from_slice(&p.net_id.to_le_bytes()[..3]);
    block[6..10].copy_from_slice(&p.dev_addr.to_le_bytes());
    block[10] = p.dl_settings;
    block[11] = p.rx_delay;

    // MIC covers MHDR and the *plaintext* body, streamed rather than concatenated.
    let mic = mic4_parts(app_key, &[&[mhdr], &block[..12]]);
    block[12..16].copy_from_slice(&mic);

    aes_decrypt_block(app_key, &mut block);

    let mut out = [0u8; JOIN_ACCEPT_LEN];
    out[0] = mhdr;
    out[1..].copy_from_slice(&block);
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

/// Which DevNonce anti-replay rule to apply — the device's LoRaWAN version decides.
///
/// 1.0.2 (and the LMIC-based Zenner fleet) draws DevNonce **randomly**, so the only
/// correct replay test is set membership over recently-accepted values: a strict
/// counter check would reject the ~half of random nonces that happen to fall below
/// the running high-water mark, making re-joins fail intermittently. 1.0.3+ made
/// DevNonce a monotonic counter, so a strict-increase check is exact there.
///
/// [`RandomWindow`](DevNoncePolicy::RandomWindow) is the safe default: because it
/// rejects only a genuine repeat, it also admits a monotonic counter's nonces
/// (each is unique and thus never in the window), so it never mis-rejects a 1.0.3+
/// device either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevNoncePolicy {
    /// 1.0.2: random DevNonce. Reject only if the value is still remembered in the
    /// window of the last `keep` accepted nonces.
    RandomWindow { keep: usize },
    /// 1.0.3/1.0.4: monotonic DevNonce. Reject unless strictly greater than the
    /// highest accepted (see [`admit_dev_nonce`]).
    Counter,
}

impl Default for DevNoncePolicy {
    fn default() -> Self {
        // The fleet runs 1.0.2; windowed is correct there and harmless for counters.
        DevNoncePolicy::RandomWindow { keep: 128 }
    }
}

/// The 1.0.2 random-DevNonce rule, in isolation: fresh unless the value is still in
/// the remembered window `recent`. `last_hi` (the running high-water, kept only for
/// diagnostics) fills the `Replay` report; it plays no part in the decision — a
/// value *below* the high-water is perfectly fresh here, which is the whole point.
pub fn admit_dev_nonce_windowed(
    recent: &[u16],
    last_hi: Option<u16>,
    seen: u16,
) -> DevNonceVerdict {
    if recent.contains(&seen) {
        DevNonceVerdict::Replay {
            last: last_hi.unwrap_or(seen),
            seen,
        }
    } else {
        DevNonceVerdict::Fresh
    }
}

/// A parsed data frame (uplink or downlink), before authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DataFrame {
    pub mhdr: u8,
    pub dev_addr: u32,
    pub fctrl: u8,
    pub fcnt: u16,
    pub fopts: FOpts,
    pub fport: Option<u8>,
    /// Still encrypted; see [`DataFrame::decrypt_payload`].
    pub frm_payload: Payload,
    mic: [u8; 4],
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
        let (fport, payload_bytes) = if body_end > header_len {
            (Some(frame[header_len]), &frame[header_len + 1..body_end])
        } else {
            (None, &frame[0..0])
        };
        // A frame longer than the PHY maximum cannot have come off a LoRaWAN radio, so
        // this rejects rather than truncates. `fopts` cannot overflow — FOptsLen is four
        // bits and FOPTS_MAX is 15 — but it is checked the same way rather than assumed.
        let fopts = FOpts::from_slice(&frame[8..header_len])
            .map_err(|_| LoRaWanError::InvalidField("FOpts exceeds the 4-bit FOptsLen field"))?;
        let frm_payload = Payload::from_slice(payload_bytes)
            .map_err(|_| LoRaWanError::InvalidField("FRMPayload exceeds the PHY maximum"))?;
        Ok(Self {
            mhdr: frame[0],
            dev_addr: u32::from_le_bytes([frame[1], frame[2], frame[3], frame[4]]),
            fctrl,
            fcnt: u16::from_le_bytes([frame[6], frame[7]]),
            fopts,
            fport,
            frm_payload,
            mic: [
                frame[body_end],
                frame[body_end + 1],
                frame[body_end + 2],
                frame[body_end + 3],
            ],
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
    /// Length of the MIC-covered region: the whole frame bar the MIC itself. Reconstructed
    /// from the parsed fields rather than stored — keeping a copy of the signed bytes
    /// alongside `fopts` and `frm_payload` duplicated up to 251 bytes of every frame.
    fn signed_len(&self) -> usize {
        // MHDR(1) DevAddr(4) FCtrl(1) FCnt(2) = 8, then the variable tail.
        8 + self.fopts.len() + usize::from(self.fport.is_some()) + self.frm_payload.len()
    }

    pub fn verify_mic(&self, nwk_skey: &[u8; 16], fcnt_full: u32) -> bool {
        let mut b0 = [0u8; 16];
        b0[0] = 0x49;
        b0[5] = if self.is_uplink() { 0 } else { 1 };
        b0[6..10].copy_from_slice(&self.dev_addr.to_le_bytes());
        b0[10..14].copy_from_slice(&fcnt_full.to_le_bytes());
        b0[15] = self.signed_len() as u8;

        // B0 || MHDR || DevAddr || FCtrl || FCnt || FOpts || [FPort] || FRMPayload,
        // streamed into CMAC in wire order instead of concatenated into a buffer.
        let dev_addr = self.dev_addr.to_le_bytes();
        let fcnt = self.fcnt.to_le_bytes();
        let fport = self.fport.map(|p| [p]).unwrap_or_default();
        let fport_part: &[u8] = if self.fport.is_some() { &fport } else { &[] };
        mic4_parts(
            nwk_skey,
            &[
                &b0,
                &[self.mhdr],
                &dev_addr,
                &[self.fctrl],
                &fcnt,
                &self.fopts,
                fport_part,
                &self.frm_payload,
            ],
        ) == self.mic
    }

    /// Decrypt FRMPayload. Uses AppSKey for application ports, NwkSKey for port 0
    /// (MAC commands) — the spec's split, and using the wrong one yields plausible
    /// garbage rather than an error.
    pub fn decrypt_payload(
        &self,
        nwk_skey: &[u8; 16],
        app_skey: &[u8; 16],
        fcnt_full: u32,
    ) -> Payload {
        let key = if self.fport == Some(0) {
            nwk_skey
        } else {
            app_skey
        };
        let dir = if self.is_uplink() { 0u8 } else { 1u8 };
        // Output is exactly as long as the input, which is already a `Payload`, so no push
        // can fail. Discarded rather than unwrapped: decryption must not be a panic path.
        let mut out = Payload::new();
        for (i, chunk) in self.frm_payload.chunks(16).enumerate() {
            let mut a = [0u8; 16];
            a[0] = 0x01;
            a[5] = dir;
            a[6..10].copy_from_slice(&self.dev_addr.to_le_bytes());
            a[10..14].copy_from_slice(&fcnt_full.to_le_bytes());
            a[15] = (i + 1) as u8;
            aes_encrypt_block(key, &mut a);
            for (b, s) in chunk.iter().zip(a.iter()) {
                let _ = out.push(b ^ s);
            }
        }
        out
    }
}

/// Parameters for a Class-A downlink data frame (network → device).
///
/// This is the minimum a single-channel responder needs to steer a joined device:
/// a MAC command in `fopts` (cleartext in 1.0.x, MIC-protected, ≤15 bytes) is enough
/// to pin the device's channel plan. `fport`/`frm_payload` are here for completeness
/// (an application downlink) and encrypt the same way an uplink does.
pub struct DownlinkParams {
    pub dev_addr: u32,
    /// NFCntDown, the network's downlink frame counter for this session. The low 16
    /// bits go on the wire; the full value binds the MIC, so a fresh session starts
    /// at 0 and the caller increments per downlink.
    pub fcnt: u32,
    /// Set the ADR bit — we manage the device's data rate, so this is normally true.
    pub adr: bool,
    /// Acknowledge a Confirmed uplink.
    pub ack: bool,
    /// More data pending (frame-pending bit).
    pub fpending: bool,
    /// MAC commands carried in the FHDR, in the clear (1.0.x). ≤15 bytes.
    pub fopts: FOpts,
    /// Application port; `None` for a MAC-only downlink (FOpts carries everything).
    pub fport: Option<u8>,
    /// Application payload; encrypted with AppSKey (or NwkSKey for port 0).
    pub frm_payload: Payload,
}

/// Build an Unconfirmed Data Down frame, MIC'd with the network session key.
///
/// The MIC uses the downlink direction bit (dir=1) in its B0 block — the same field
/// [`DataFrame::verify_mic`] checks — so a frame built here round-trips through
/// [`DataFrame::parse`] + `verify_mic`. FOpts are transmitted in the clear (LoRaWAN
/// 1.0.x); only `frm_payload` is encrypted.
pub fn build_data_down(
    nwk_skey: &[u8; 16],
    app_skey: &[u8; 16],
    p: &DownlinkParams,
) -> Result<Frame, LoRaWanError> {
    // FOptsLen is a 4-bit field, so anything longer cannot be represented on air. The
    // `FOpts` type now enforces this at construction, but the check stays: a type bound
    // and a protocol rule agreeing is not a reason to stop stating the rule, and this is
    // the error a caller reads. It was originally an `assert!`, i.e. a panic in a library
    // called from a gateway that must not die because a caller built an over-long command.
    if p.fopts.len() > FOPTS_MAX {
        return Err(LoRaWanError::InvalidField(
            "FOpts exceeds the 4-bit FOptsLen field",
        ));
    }
    // FOpts and FRMPayload are individually within their limits, but their *sum* plus the
    // header and MIC can still exceed the PHY maximum (12 + 15 + 242 = 269). That is a
    // real, reachable overflow, so it is refused here rather than truncated on air.
    if 12 + p.fopts.len() + p.frm_payload.len() > PHY_PAYLOAD_MAX {
        return Err(LoRaWanError::InvalidField("frame exceeds the PHY maximum"));
    }
    let mhdr = MTYPE_UNCONFIRMED_DOWN << 5;
    let fctrl = (if p.adr { 0x80 } else { 0 })
        | (if p.ack { 0x20 } else { 0 })
        | (if p.fpending { 0x10 } else { 0 })
        | (p.fopts.len() as u8 & 0x0F);

    // Every push below is inside the capacity the length check above just proved, so the
    // Results cannot be Err. They are discarded rather than unwrapped so that no path
    // through a frame builder can panic; a caller that exceeds the limit already got the
    // explicit error.
    let mut frame = Frame::new();
    let _ = frame.push(mhdr);
    let _ = frame.extend_from_slice(&p.dev_addr.to_le_bytes());
    let _ = frame.push(fctrl);
    let _ = frame.extend_from_slice(&(p.fcnt as u16).to_le_bytes());
    let _ = frame.extend_from_slice(&p.fopts);
    if let Some(port) = p.fport {
        let _ = frame.push(port);
        let key = if port == 0 { nwk_skey } else { app_skey };
        for (i, chunk) in p.frm_payload.chunks(16).enumerate() {
            let mut a = [0u8; 16];
            a[0] = 0x01;
            a[5] = 1; // downlink direction
            a[6..10].copy_from_slice(&p.dev_addr.to_le_bytes());
            a[10..14].copy_from_slice(&p.fcnt.to_le_bytes());
            a[15] = (i + 1) as u8;
            aes_encrypt_block(key, &mut a);
            for (b, s) in chunk.iter().zip(a.iter()) {
                let _ = frame.push(b ^ s);
            }
        }
    }

    // MIC over B0(dir=1) || frame, per EN/LoRaWAN 1.0.x — streamed, not concatenated.
    let mut b0 = [0u8; 16];
    b0[0] = 0x49;
    b0[5] = 1; // downlink
    b0[6..10].copy_from_slice(&p.dev_addr.to_le_bytes());
    b0[10..14].copy_from_slice(&p.fcnt.to_le_bytes());
    b0[15] = frame.len() as u8;
    let mic = mic4_parts(nwk_skey, &[&b0, &frame]);
    let _ = frame.extend_from_slice(&mic);
    Ok(frame)
}

/// The `LinkADRReq` MAC command (CID 0x03), as the 5 FOpts bytes.
///
/// `channel_mask` bit *n* enables channel *n* (bit 0 = EU868 868.1 MHz), so
/// `0x0001` pins the device to the single default channel a one-channel gateway
/// hears. `data_rate` and `tx_power` use the spec's `0x0F` sentinel to mean "keep
/// current", so pinning the channel need not disturb the device's DR or power.
/// `nb_trans` is the uplink repetition count (1 = default). ChMaskCntl is 0 — the
/// mask applies to channels 0–15.
pub fn link_adr_req(channel_mask: u16, data_rate: u8, tx_power: u8, nb_trans: u8) -> [u8; 5] {
    [
        0x03,
        ((data_rate & 0x0F) << 4) | (tx_power & 0x0F),
        (channel_mask & 0xFF) as u8,
        (channel_mask >> 8) as u8,
        nb_trans & 0x0F, // Redundancy: ChMaskCntl=0 (bits 6:4), NbTrans (bits 3:0)
    ]
}

/// Scan an uplink's FOpts for a `LinkADRAns` (CID 0x03) and report whether the device
/// accepted all three fields (channel-mask, data-rate, power).
///
/// FOpts is a back-to-back sequence of MAC commands, so this walks it using the
/// known uplink command lengths rather than assuming position; an unrecognised CID
/// stops the walk (we cannot know its length to skip it). Returns `None` if no
/// LinkADRAns is present.
pub fn parse_link_adr_ans(fopts: &[u8]) -> Option<bool> {
    let mut i = 0;
    while i < fopts.len() {
        let cid = fopts[i];
        // Payload length (excluding the CID byte) of each uplink MAC answer.
        let payload_len = match cid {
            0x02 => 0,        // LinkCheckReq
            0x03 => 1,        // LinkADRAns  ← the one we want
            0x04 => 0,        // DutyCycleAns
            0x05 => 1,        // RXParamSetupAns
            0x06 => 2,        // DevStatusAns
            0x07 => 1,        // NewChannelAns
            0x08 => 0,        // RXTimingSetupAns
            0x09 => 0,        // TxParamSetupAns
            0x0A => 1,        // DlChannelAns
            _ => return None, // unknown CID: cannot walk further
        };
        if cid == 0x03 {
            let status = *fopts.get(i + 1)?;
            // bit0 ChannelMaskACK, bit1 DataRateACK, bit2 PowerACK
            return Some(status & 0x07 == 0x07);
        }
        i += 1 + payload_len;
    }
    None
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
            fopts: FOpts::new(),
            fport: Some(1),
            frm_payload: Payload::from_slice(payload).unwrap(),
            mic: [0; 4],
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
        assert_eq!(&f.fopts[..], &[0xAA, 0xBB, 0xCC][..]);
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
    fn counter_rule_spuriously_rejects_a_fresh_random_nonce() {
        // The defect, in isolation: a 1.0.2 device draws DevNonce randomly, so a
        // fresh value can land *below* the running high-water. The counter rule
        // (1.0.4) wrongly calls that a replay — this is exactly what would strand a
        // real meter on re-join.
        assert_eq!(
            admit_dev_nonce(Some(40_000), 12_000),
            DevNonceVerdict::Replay {
                last: 40_000,
                seen: 12_000
            },
        );
        // The windowed rule (1.0.2) admits it — it was never used before.
        let recent = [40_000u16];
        assert_eq!(
            admit_dev_nonce_windowed(&recent, Some(40_000), 12_000),
            DevNonceVerdict::Fresh,
        );
        // But a genuine repeat is still refused.
        assert_eq!(
            admit_dev_nonce_windowed(&recent, Some(40_000), 40_000),
            DevNonceVerdict::Replay {
                last: 40_000,
                seen: 40_000
            },
        );
    }

    #[test]
    fn link_adr_req_pins_channel_zero_without_touching_dr_or_power() {
        // ChMask=0x0001 (only 868.1), DR/power = "no change" (0xF), NbTrans=1.
        assert_eq!(
            link_adr_req(0x0001, 0x0F, 0x0F, 1),
            [0x03, 0xFF, 0x01, 0x00, 0x01]
        );
    }

    #[test]
    fn data_down_with_link_adr_req_round_trips_and_mic_verifies() {
        let fopts = FOpts::from_slice(&link_adr_req(0x0001, 0x0F, 0x0F, 1)).unwrap();
        let p = DownlinkParams {
            dev_addr: 0x2600_0001,
            fcnt: 0,
            adr: true,
            ack: false,
            fpending: false,
            fopts: fopts.clone(),
            fport: None,
            frm_payload: Payload::new(),
        };
        let frame = build_data_down(&NWK_SKEY, &APP_SKEY, &p).expect("valid params");

        // It must parse as a *downlink* and authenticate with dir=1 under NwkSKey.
        let df = DataFrame::parse(&frame).unwrap();
        assert!(!df.is_uplink());
        assert_eq!(df.dev_addr, 0x2600_0001);
        assert_eq!(df.fcnt, 0);
        assert_eq!(df.fopts, fopts); // FOpts carried in the clear (1.0.x)
        assert_eq!(df.fport, None);
        assert!(df.verify_mic(&NWK_SKEY, 0));
        // Wrong counter must fail the MIC just like a forgery.
        assert!(!df.verify_mic(&NWK_SKEY, 1));
    }

    #[test]
    fn data_down_app_payload_round_trips_through_decrypt() {
        let payload = b"pin-check".to_vec();
        let p = DownlinkParams {
            dev_addr: 0x2600_0002,
            fcnt: 7,
            adr: true,
            ack: false,
            fpending: false,
            fopts: FOpts::new(),
            fport: Some(1),
            frm_payload: Payload::from_slice(&payload).unwrap(),
        };
        let frame = build_data_down(&NWK_SKEY, &APP_SKEY, &p).expect("valid params");
        let df = DataFrame::parse(&frame).unwrap();
        assert!(df.verify_mic(&NWK_SKEY, 7));
        // Decrypting the downlink with the same keys/counter recovers the plaintext.
        assert_eq!(
            &df.decrypt_payload(&NWK_SKEY, &APP_SKEY, 7)[..],
            &payload[..]
        );
    }

    #[test]
    fn over_long_fopts_cannot_be_constructed_at_all() {
        // This was an `assert!` (a panic in a library a gateway links against), then a
        // runtime `Err`. With `FOpts` fixed at FOPTS_MAX the invalid state is now
        // unrepresentable: the 4-bit FOptsLen limit is enforced by the type, so the error
        // moves from build time to construction time and cannot be bypassed by a caller
        // who ignores a Result.
        assert!(FOpts::from_slice(&[0u8; 16]).is_err());
        // 15 is the boundary and must still succeed, end to end.
        let ok = DownlinkParams {
            dev_addr: 0x2600_0001,
            fcnt: 0,
            adr: true,
            ack: false,
            fpending: false,
            fopts: FOpts::from_slice(&[0u8; 15]).unwrap(),
            fport: None,
            frm_payload: Payload::new(),
        };
        assert!(build_data_down(&NWK_SKEY, &APP_SKEY, &ok).is_ok());
    }

    #[test]
    fn a_frame_over_the_phy_maximum_is_an_error_not_a_truncation() {
        // FOpts and FRMPayload are each individually legal, but 12 + 15 + 242 = 269 > 255.
        // This is the one overflow the type system cannot catch, so it must be refused
        // rather than silently truncated onto the air.
        let p = DownlinkParams {
            dev_addr: 0x2600_0001,
            fcnt: 0,
            adr: true,
            ack: false,
            fpending: false,
            fopts: FOpts::from_slice(&[0u8; 15]).unwrap(),
            fport: Some(1),
            frm_payload: Payload::from_slice(&[0u8; FRM_PAYLOAD_MAX]).unwrap(),
        };
        assert_eq!(
            build_data_down(&NWK_SKEY, &APP_SKEY, &p),
            Err(LoRaWanError::InvalidField("frame exceeds the PHY maximum"))
        );
    }

    #[test]
    fn parse_link_adr_ans_reads_the_accept_bits() {
        // status 0x07 = all three ACK bits set → accepted.
        assert_eq!(parse_link_adr_ans(&[0x03, 0x07]), Some(true));
        // ChannelMaskACK (bit0) clear → not fully accepted.
        assert_eq!(parse_link_adr_ans(&[0x03, 0x06]), Some(false));
        // No LinkADRAns present.
        assert_eq!(parse_link_adr_ans(&[]), None);
        // Walk past a preceding known command (DevStatusAns, 2 bytes) to find it.
        assert_eq!(
            parse_link_adr_ans(&[0x06, 0x00, 0x00, 0x03, 0x07]),
            Some(true)
        );
        // Unknown leading CID stops the walk safely.
        assert_eq!(parse_link_adr_ans(&[0x7F, 0x03, 0x07]), None);
    }

    // --- Downlink / MAC-command (channel pin) ---

    const NWK_SKEY: [u8; 16] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10,
    ];
    const APP_SKEY: [u8; 16] = [
        0xF0, 0xE0, 0xD0, 0xC0, 0xB0, 0xA0, 0x90, 0x80, 0x70, 0x60, 0x50, 0x40, 0x30, 0x20, 0x10,
        0x00,
    ];
}
