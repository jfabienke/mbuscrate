//! Passive weak-key / weak-crypto audit over captured wM-Bus frames.
//!
//! This is a *defensive fleet-hygiene* tool, not a key-recovery attack. It answers
//! one question per meter — "is this meter trivially readable by a passive
//! listener?" — using only captured bytes and *published* (non-secret) default
//! keys. It cannot break a full-entropy key: there is deliberately no exhaustive
//! search, only a fixed candidate list of known defaults.
//!
//! Two arms, matched to the two ciphers actually seen in the field:
//!
//! * **Arm 1 — OMS mode-5 CBC** (CI `0x7A` short / `0x72` long, Zenner-style).
//!   Tries each published default key and confirms with the `2F 2F` idle-fill
//!   oracle ([`crate::wmbus::oms::decrypted_ok`]). A hit means the meter is still
//!   on a factory default and is readable by anyone holding that public constant.
//!
//! * **Arm 2 — ELL AES-128-CTR** (CI `0x8D`/`0x8F`, Kamstrup-style). There is no
//!   published fleet-wide Kamstrup default, so the default-key question does not
//!   apply. Instead this arm runs the two *key-free* checks that matter for CTR:
//!   flag meters transmitting in the clear (`ENC == 0`), and flag session-number
//!   reuse — the catastrophic CTR failure mode, where a repeated `(key, IV)` makes
//!   the keystream recoverable without ever knowing the key.
//!
//! Frames are the CRC-stripped, L-prefixed bytes the decoder receives (the
//! `metermon-rs capture` format: `L C M M ID ID ID ID ver type CI …`).

#![cfg(feature = "crypto")]

use std::collections::HashMap;

use crate::vendors::manufacturer::id_to_manufacturer;
use crate::wmbus::crypto::AesKey;
use crate::wmbus::ell::{parse_ell, EllSecurity, CI_ELL_I, CI_ELL_II, CI_ELL_III, CI_ELL_IV};
use crate::wmbus::oms::{decrypt_mode5_cbc, decrypted_ok};

/// Published, non-secret default keys to test in arm 1. These ship in vendor
/// software; testing them is the whole point of the audit.
pub fn default_keys() -> Vec<(&'static str, AesKey)> {
    vec![
        // Zenner "ZDK" — ZR_ClassLibrary/AES.cs, a published constant.
        ("ZENNER_ZDK", AesKey::from_hex("5A8470C4806F4A87CEF4D5F2D985AB18").unwrap()),
        // The all-zero key: "encryption off" sentinel, occasionally left installed.
        ("ALL_ZERO", AesKey::from_hex("00000000000000000000000000000000").unwrap()),
    ]
}

/// Transport/link security profile detected for a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Profile {
    Mode5Cbc,
    EllCtr,
    EllPlain,
    /// A CI we do not classify as either encrypted profile.
    Unclassified(u8),
}

/// Per-meter audit verdict, worst-finding-first in [`severity`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Arm 1 hit: decrypts under a published default key. Readable by anyone.
    DefaultKey(&'static str),
    /// Arm 2: ELL header declares the payload cleartext. Readable by anyone.
    Plaintext,
    /// Arm 2: an ELL session number repeated under the same meter — CTR keystream
    /// reuse, plaintext recoverable without the key.
    SessionReuse { sn: u32 },
    /// Mode-5 meter that matched no tested default — good (not provably strong).
    NoDefaultKeyMatch,
    /// ELL-CTR meter with no key-free weakness found — good (not provably strong).
    EncryptedNoWeakness,
    /// Profile we cannot audit from these captures.
    Unaudited(&'static str),
}

impl Verdict {
    /// Higher = more urgent. Used to keep the worst finding per meter.
    pub fn severity(&self) -> u8 {
        match self {
            Verdict::DefaultKey(_) => 4,
            Verdict::Plaintext => 3,
            Verdict::SessionReuse { .. } => 3,
            Verdict::Unaudited(_) => 1,
            Verdict::NoDefaultKeyMatch | Verdict::EncryptedNoWeakness => 0,
        }
    }
    pub fn is_exposure(&self) -> bool {
        matches!(
            self,
            Verdict::DefaultKey(_) | Verdict::Plaintext | Verdict::SessionReuse { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeterVerdict {
    pub serial: u32,
    pub mfr: String,
    pub profile: Profile,
    pub verdict: Verdict,
    pub frames_seen: u32,
}

/// wM-Bus link-layer header, present on every frame regardless of encryption.
struct LinkHeader {
    mfr_id: u16,
    serial: u32,
    link_address: [u8; 8],
    ci: u8,
    after_ci_offset: usize,
}

/// Decode BCD ID (4 bytes, little-endian) to its integer serial.
fn bcd_le_serial(b: &[u8]) -> u32 {
    b.iter().rev().fold(0u32, |acc, &byte| {
        acc * 100 + (byte >> 4) as u32 * 10 + (byte & 0x0F) as u32
    })
}

fn parse_link_header(raw: &[u8]) -> Option<LinkHeader> {
    // L C | M M | ID ID ID ID | ver type | CI …
    if raw.len() < 11 {
        return None;
    }
    let mut link_address = [0u8; 8];
    link_address.copy_from_slice(&raw[2..10]); // M(2) ID(4) ver(1) type(1)
    Some(LinkHeader {
        mfr_id: u16::from_le_bytes([raw[2], raw[3]]),
        serial: bcd_le_serial(&raw[4..8]),
        link_address,
        ci: raw[10],
        after_ci_offset: 11,
    })
}

/// One frame's contribution to the audit: profile, an optional arm-1 default hit,
/// and (for ELL-CTR) the session number, so the aggregator can spot reuse.
struct FrameFinding {
    serial: u32,
    mfr: String,
    profile: Profile,
    default_hit: Option<&'static str>,
    plaintext: bool,
    session_number: Option<u32>,
}

fn audit_frame(raw: &[u8], keys: &[(&'static str, AesKey)]) -> Option<FrameFinding> {
    let h = parse_link_header(raw)?;
    let mfr = id_to_manufacturer(h.mfr_id);
    let after_ci = &raw[h.after_ci_offset..];

    match h.ci {
        // ---- Arm 1: OMS mode-5 CBC ----
        0x7A | 0x72 => {
            // short header: ACC STATUS CFG(2); long header: 8-byte TPL addr first.
            let addr_prefix = if h.ci == 0x72 { 8 } else { 0 };
            // acc, then status(1), config(2), then ciphertext.
            let ct_start = addr_prefix + 4;
            if after_ci.len() <= ct_start {
                return Some(FrameFinding {
                    serial: h.serial, mfr, profile: Profile::Mode5Cbc,
                    default_hit: None, plaintext: false, session_number: None,
                });
            }
            let acc = after_ci[addr_prefix];
            let ct = &after_ci[ct_start..];
            // Encrypted region is a multiple of the 16-byte block; trim any trailing
            // stray bytes (e.g. an un-stripped CRC) rather than fail outright.
            let usable = ct.len() - (ct.len() % 16);
            let mut hit = None;
            if usable >= 16 {
                for (name, key) in keys {
                    if let Ok(pt) = decrypt_mode5_cbc(&ct[..usable], &h.link_address, acc, key) {
                        if decrypted_ok(&pt) {
                            hit = Some(*name);
                            break;
                        }
                    }
                }
            }
            Some(FrameFinding {
                serial: h.serial, mfr, profile: Profile::Mode5Cbc,
                default_hit: hit, plaintext: false, session_number: None,
            })
        }
        // ---- Arm 2: ELL AES-CTR ----
        CI_ELL_I | CI_ELL_II | CI_ELL_III | CI_ELL_IV => {
            match parse_ell(&raw[10..]) {
                Ok(hdr) => {
                    let (profile, plaintext) = match hdr.security {
                        EllSecurity::None => (Profile::EllPlain, true),
                        EllSecurity::Aes128Ctr => (Profile::EllCtr, false),
                        EllSecurity::Reserved(_) => (Profile::Unclassified(h.ci), false),
                    };
                    Some(FrameFinding {
                        serial: h.serial, mfr, profile,
                        default_hit: None, plaintext,
                        session_number: hdr.session_number,
                    })
                }
                Err(_) => Some(FrameFinding {
                    serial: h.serial, mfr, profile: Profile::Unclassified(h.ci),
                    default_hit: None, plaintext: false, session_number: None,
                }),
            }
        }
        other => Some(FrameFinding {
            serial: h.serial, mfr, profile: Profile::Unclassified(other),
            default_hit: None, plaintext: false, session_number: None,
        }),
    }
}

/// Aggregate per-meter state while walking frames.
struct MeterState {
    mfr: String,
    profile: Profile,
    frames: u32,
    default_hit: Option<&'static str>,
    plaintext: bool,
    seen_sns: HashMap<u32, u32>, // sn -> count, for CTR reuse detection
}

/// Audit a whole capture. Returns one verdict per meter, sorted worst-first.
pub fn audit_capture(frames: &[Vec<u8>]) -> Vec<MeterVerdict> {
    let keys = default_keys();
    let mut states: HashMap<u32, MeterState> = HashMap::new();

    for raw in frames {
        let Some(f) = audit_frame(raw, &keys) else { continue };
        let st = states.entry(f.serial).or_insert_with(|| MeterState {
            mfr: f.mfr.clone(),
            profile: f.profile.clone(),
            frames: 0,
            default_hit: None,
            plaintext: false,
            seen_sns: HashMap::new(),
        });
        st.frames += 1;
        st.profile = f.profile.clone();
        if let Some(name) = f.default_hit {
            st.default_hit = Some(name);
        }
        st.plaintext |= f.plaintext;
        if let Some(sn) = f.session_number {
            *st.seen_sns.entry(sn).or_insert(0) += 1;
        }
    }

    let mut out: Vec<MeterVerdict> = states
        .into_iter()
        .map(|(serial, st)| {
            let reused_sn = st.seen_sns.iter().find(|(_, &c)| c > 1).map(|(&sn, _)| sn);
            let verdict = if let Some(name) = st.default_hit {
                Verdict::DefaultKey(name)
            } else if st.plaintext {
                Verdict::Plaintext
            } else if let Some(sn) = reused_sn {
                Verdict::SessionReuse { sn }
            } else {
                match st.profile {
                    Profile::Mode5Cbc => Verdict::NoDefaultKeyMatch,
                    Profile::EllCtr => Verdict::EncryptedNoWeakness,
                    Profile::EllPlain => Verdict::Plaintext,
                    Profile::Unclassified(_) => Verdict::Unaudited("unclassified CI"),
                }
            };
            MeterVerdict {
                serial, mfr: st.mfr, profile: st.profile, verdict, frames_seen: st.frames,
            }
        })
        .collect();

    out.sort_by(|a, b| {
        b.verdict
            .severity()
            .cmp(&a.verdict.severity())
            .then(a.serial.cmp(&b.serial))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{BlockEncryptMut, KeyIvInit};

    const ZDK: &str = "5A8470C4806F4A87CEF4D5F2D985AB18";

    /// Encrypt a plaintext block set under mode-5 CBC (mirror of the oms.rs helper).
    fn mode5_encrypt(plaintext: &[u8], link: &[u8; 8], acc: u8, key: &AesKey) -> Vec<u8> {
        let iv = crate::wmbus::oms::mode5_cbc_iv(link, acc);
        let mut buf = plaintext.to_vec();
        cbc::Encryptor::<aes::Aes128>::new(key.as_bytes().into(), &iv.into())
            .encrypt_padded_mut::<cbc::cipher::block_padding::NoPadding>(&mut buf, plaintext.len())
            .unwrap()
            .to_vec()
    }

    /// Build a mode-5 short-header (CI 0x7A) frame around a given ciphertext.
    fn mode5_frame(link: &[u8; 8], acc: u8, ct: &[u8]) -> Vec<u8> {
        let mut f = vec![0u8]; // L (value irrelevant to the audit)
        f.push(0x44); // C
        f.extend_from_slice(link); // M ID ver type
        f.push(0x7A); // CI short header
        f.push(acc);
        f.push(0x00); // status
        f.extend_from_slice(&[0x00, 0x05]); // config word (mode 5)
        f.extend_from_slice(ct);
        f
    }

    /// Build an ELL-II (CI 0x8D) frame with a chosen session number.
    fn ell2_frame(link: &[u8; 8], cc: u8, acc: u8, sn: u32, body: &[u8]) -> Vec<u8> {
        let mut f = vec![0u8, 0x44];
        f.extend_from_slice(link);
        f.push(CI_ELL_II);
        f.push(cc);
        f.push(acc);
        f.extend_from_slice(&sn.to_le_bytes());
        f.extend_from_slice(body);
        f
    }

    // link_address for a synthetic meter, serial 55298170 (BCD 70 81 29 55).
    fn link() -> [u8; 8] {
        [0x2D, 0x2C, 0x70, 0x81, 0x29, 0x55, 0x01, 0x07] // KAM mfr, ID, ver, type
    }

    #[test]
    fn arm1_flags_a_meter_on_the_zenner_default() {
        let key = AesKey::from_hex(ZDK).unwrap();
        let plain = [0x2Fu8; 16]; // valid mode-5 plaintext: idle-fill
        let acc = 0x2A;
        let ct = mode5_encrypt(&plain, &link(), acc, &key);
        let frame = mode5_frame(&link(), acc, &ct);

        let v = audit_capture(&[frame]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].verdict, Verdict::DefaultKey("ZENNER_ZDK"));
        assert!(v[0].verdict.is_exposure());
    }

    #[test]
    fn arm1_clears_a_meter_on_a_random_key() {
        let key = AesKey::from_hex("00112233445566778899AABBCCDDEEFF").unwrap();
        let plain = [0x2Fu8; 16];
        let acc = 0x11;
        let ct = mode5_encrypt(&plain, &link(), acc, &key);
        let frame = mode5_frame(&link(), acc, &ct);

        let v = audit_capture(&[frame]);
        assert_eq!(v[0].verdict, Verdict::NoDefaultKeyMatch);
        assert!(!v[0].verdict.is_exposure());
    }

    #[test]
    fn arm2_flags_a_plaintext_ell_meter() {
        // SN with top 3 bits = 000 => EllSecurity::None => cleartext.
        let sn = 0x0000_0042;
        let frame = ell2_frame(&link(), 0x00, 0x2A, sn, &[0x0C, 0x13, 0x00, 0x00]);
        let v = audit_capture(&[frame]);
        assert_eq!(v[0].verdict, Verdict::Plaintext);
        assert!(v[0].verdict.is_exposure());
    }

    #[test]
    fn arm2_flags_ctr_session_number_reuse() {
        // SN with top 3 bits = 001 => AES-128-CTR. Same SN twice => keystream reuse.
        let sn = 0x2000_0007;
        let f1 = ell2_frame(&link(), 0x00, 0x01, sn, &[0xDE, 0xAD]);
        let f2 = ell2_frame(&link(), 0x00, 0x02, sn, &[0xBE, 0xEF]);
        let v = audit_capture(&[f1, f2]);
        assert_eq!(v[0].verdict, Verdict::SessionReuse { sn });
        assert_eq!(v[0].frames_seen, 2);
    }

    #[test]
    fn arm2_clears_ctr_with_distinct_session_numbers() {
        let f1 = ell2_frame(&link(), 0x00, 0x01, 0x2000_0007, &[0xDE, 0xAD]);
        let f2 = ell2_frame(&link(), 0x00, 0x02, 0x2000_0008, &[0xBE, 0xEF]);
        let v = audit_capture(&[f1, f2]);
        assert_eq!(v[0].verdict, Verdict::EncryptedNoWeakness);
        assert!(!v[0].verdict.is_exposure());
    }

    #[test]
    fn bcd_serial_decodes() {
        assert_eq!(bcd_le_serial(&[0x70, 0x81, 0x29, 0x55]), 55298170);
    }
}
