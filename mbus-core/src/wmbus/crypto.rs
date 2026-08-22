//! Key material and wire-format errors for wM-Bus cryptography.
//!
//! The *algorithms* live above this in `mbus-rs` for now; what moved here is what every
//! mode needs and nothing else can be written without: the key type, the device identity
//! used to derive per-device keys and IVs, and an allocation-free error.
//!
//! [`CryptoError`] deliberately carries no `String`. The five `String`-bearing variants in
//! `mbus-rs`'s version were prose — "GCM authentication/decryption failed" tells a caller
//! nothing the variant name does not — and they were the most viral allocation in the
//! module, since every `Result` in it carried one.

use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Something went wrong encrypting, decrypting or deriving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CryptoError {
    /// Key was not 16 bytes.
    InvalidKeyLength { expected: usize, actual: usize },
    /// Ciphertext length is not a whole number of cipher blocks.
    InvalidDataLength { block_size: usize, actual: usize },
    /// Security mode is not one this crate implements.
    UnsupportedMode { mode: u8 },
    /// The initialisation vector could not be built from the frame.
    InvalidIv,
    /// Authentication failed, or the ciphertext was malformed.
    ///
    /// Deliberately does not say which. Distinguishing "bad tag" from "bad padding" to a
    /// caller is how padding oracles are built.
    DecryptionFailed,
    /// Encryption failed.
    EncryptionFailed,
    /// The frame was not shaped the way the security mode requires.
    InvalidFrame(&'static str),
}

impl core::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidKeyLength { expected, actual } => {
                write!(f, "invalid key length: expected {expected}, got {actual}")
            }
            Self::InvalidDataLength { block_size, actual } => write!(
                f,
                "data length {actual} is not a multiple of the {block_size}-byte block"
            ),
            Self::UnsupportedMode { mode } => write!(f, "unsupported security mode {mode}"),
            Self::InvalidIv => write!(f, "invalid initialisation vector"),
            Self::DecryptionFailed => write!(f, "decryption failed"),
            Self::EncryptionFailed => write!(f, "encryption failed"),
            Self::InvalidFrame(why) => write!(f, "invalid frame: {why}"),
        }
    }
}

impl core::error::Error for CryptoError {}

/// AES-128 key.
///
/// `PartialEq` and `Debug` are hand-written, because a derive gets both wrong on a secret:
/// the derived comparison returns at the first differing byte, so its duration reveals how
/// many leading bytes matched, and the derived `Debug` would print the key straight into a
/// log file — defeating the point of `ZeroizeOnDrop`.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct AesKey {
    key: [u8; 16],
}

impl AesKey {
    /// Wrap 16 bytes as a key.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 16 {
            return Err(CryptoError::InvalidKeyLength {
                expected: 16,
                actual: bytes.len(),
            });
        }
        let mut key = [0u8; 16];
        key.copy_from_slice(bytes);
        Ok(Self { key })
    }

    /// Parse a key from 32 hex digits.
    ///
    /// Allocation-free: `mbus-rs`'s `util::hex` builds a `String`, but a key is a fixed
    /// 16 bytes, so the length is known and the digits can be folded in place.
    pub fn from_hex(hex_str: &str) -> Result<Self, CryptoError> {
        let b = hex_str.as_bytes();
        if b.len() != 32 {
            return Err(CryptoError::InvalidKeyLength {
                expected: 32,
                actual: b.len(),
            });
        }
        let digit = |c: u8| -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        };
        let mut key = [0u8; 16];
        for (i, slot) in key.iter_mut().enumerate() {
            let hi = digit(b[i * 2]).ok_or(CryptoError::InvalidFrame("key is not hex"))?;
            let lo = digit(b[i * 2 + 1]).ok_or(CryptoError::InvalidFrame("key is not hex"))?;
            *slot = (hi << 4) | lo;
        }
        Ok(Self { key })
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.key
    }
}

impl PartialEq for AesKey {
    fn eq(&self, other: &Self) -> bool {
        self.key.ct_eq(&other.key).into()
    }
}

impl Eq for AesKey {}

impl core::fmt::Debug for AesKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AesKey(<redacted>)")
    }
}

/// Where a device's key comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum KeyMode {
    /// The supplied key IS the device key, byte for byte. This is how real meters ship:
    /// each is provisioned with its own key.
    #[default]
    Direct,
    /// Derive a per-device key from a master key.
    ///
    /// **Not implemented here on purpose.** OMS Vol.2 specifies an AES-CMAC derivation
    /// whose exact input encoding this crate has never had to hand, and a
    /// plausible-looking wrong KDF is worse than an absent one — it produces keys that
    /// look fine and decrypt nothing.
    DerivedFromMaster,
}

/// Identity fields a device's IV and key derivation are built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DeviceInfo {
    pub device_id: u32,
    pub manufacturer: u16,
    pub version: u8,
    pub device_type: u8,
    /// Access number from the frame, used in Mode 9 IV construction.
    pub access_number: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_never_renders_its_bytes() {
        let k = AesKey::from_bytes(&[0xAB; 16]).unwrap();
        let shown = format!("{k:?}");
        assert!(!shown.to_lowercase().contains("ab"), "got: {shown}");
        assert!(!shown.contains("171"), "got: {shown}");
    }

    #[test]
    fn keys_compare_by_value_in_constant_time() {
        let a = AesKey::from_bytes(&[1u8; 16]).unwrap();
        let b = AesKey::from_bytes(&[1u8; 16]).unwrap();
        let c = AesKey::from_bytes(&[2u8; 16]).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        // Differing only in the LAST byte must still compare unequal — a short-circuiting
        // comparison would get this right too, but it is the case that distinguishes a
        // correct constant-time implementation from one that stops early and returns true.
        let mut tail = [1u8; 16];
        tail[15] = 9;
        assert_ne!(a, AesKey::from_bytes(&tail).unwrap());
    }

    #[test]
    fn a_wrong_length_key_is_refused_with_both_lengths() {
        assert_eq!(
            AesKey::from_bytes(&[0u8; 8]),
            Err(CryptoError::InvalidKeyLength {
                expected: 16,
                actual: 8
            })
        );
    }

    #[test]
    fn from_hex_accepts_either_case_and_rejects_the_rest() {
        let lower = AesKey::from_hex("0123456789abcdef0123456789abcdef").unwrap();
        let upper = AesKey::from_hex("0123456789ABCDEF0123456789ABCDEF").unwrap();
        assert_eq!(lower, upper);
        assert_eq!(lower.as_bytes()[0], 0x01);
        assert_eq!(lower.as_bytes()[15], 0xEF);

        assert!(AesKey::from_hex("0123").is_err(), "too short");
        assert!(
            AesKey::from_hex("0123456789abcdef0123456789abcdeg").is_err(),
            "'g' is not a hex digit"
        );
    }

    #[test]
    fn direct_is_the_default_key_mode() {
        // Real meters are provisioned per device, so treating the supplied key as the
        // device key is the behaviour that works; derivation is the special case.
        assert_eq!(KeyMode::default(), KeyMode::Direct);
    }
}
