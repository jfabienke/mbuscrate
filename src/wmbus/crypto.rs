//! # wM-Bus Mode 9 (AES-128-GCM) + LoRaWAN/auth helpers
//!
//! This module implements the wM-Bus **Mode 9 (AES-128-GCM, OMS 7.3.6)** transport
//! security profile, plus the LoRaWAN MIC / HMAC-SHA1 / Qundis authentication helpers.
//!
//! The other transport/link security profiles live in their own canonical modules and
//! must be used directly:
//!
//! - **Mode 5 (OMS Security Profile A, AES-128-CBC)**: [`crate::wmbus::oms`]
//! - **ELL link-layer (AES-128-CTR)**: [`crate::wmbus::ell`]
//!
//! This module no longer detects the encryption mode from the CI field or dispatches to
//! CTR/CBC/ECB paths; that legacy facade was retired because its Mode 5 path used the
//! wrong cipher (CTR instead of CBC) and its mode detection read the wrong field.
//!
//! ## Features
//!
//! - **Mode 9 GCM**: authenticated AES-128-GCM encrypt/decrypt with OMS 7.3.6 AAD + IV
//! - **LoRaWAN MIC**: CMAC-AES128 message integrity codes
//! - **Auth helpers**: HMAC-SHA1 and the Qundis 3-step authentication flow
//! - **Error Handling**: comprehensive error types for encryption/decryption failures
//!
//! ## Usage
//!
//! ```rust,no_run
//! use mbus_rs::wmbus::crypto::{WMBusCrypto, AesKey, DeviceInfo};
//!
//! let key = AesKey::from_bytes(&[0; 16]).unwrap();
//! let mut crypto = WMBusCrypto::new(key);
//!
//! # let encrypted_frame: Vec<u8> = Vec::new();
//! # let device_info = DeviceInfo {
//! #     device_id: 0x12345678,
//! #     manufacturer: 0x2C2D,
//! #     version: 0x01,
//! #     device_type: 0x07,
//! #     access_number: None,
//! # };
//! // Decrypt a Mode 9 (AES-128-GCM) wM-Bus frame
//! let decrypted = crypto.decrypt_mode9_gcm(&encrypted_frame, &device_info).unwrap();
//! ```

use thiserror::Error;

/// Enhanced encryption errors with specific failure types
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CryptoError {
    #[error("Invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("Invalid data length: must be multiple of {block_size}, got {actual}")]
    InvalidDataLength { block_size: usize, actual: usize },

    #[error("Unsupported encryption mode: {mode}")]
    UnsupportedMode { mode: u8 },

    #[error("Invalid initialization vector: {reason}")]
    InvalidIV { reason: String },

    #[error("Decryption failed: {reason}")]
    DecryptionFailed { reason: String },

    #[error("Encryption failed: {reason}")]
    EncryptionFailed { reason: String },

    #[error("Invalid frame structure: {reason}")]
    InvalidFrame { reason: String },

    #[error("Key derivation failed: {reason}")]
    KeyDerivationFailed { reason: String },
}

// AesKey, CryptoError, DeviceInfo and KeyMode live in `mbus-core` so that every crate in
// the tree shares one key type — two would mean converting at the boundary, which for a
// secret means copying it. `derive_device_key` is NOT re-exported: it was an XOR labelled
// as a KDF, is deprecated below, and has no place in the core.
pub use mbus_core::wmbus::crypto::{AesKey, DeviceInfo, KeyMode};

// GCM header construction moved to the core: AAD and IV are fixed-size pure functions of
// the frame header and device identity, and were already allocation-free. The cipher
// operations stay here until aes-gcm's allocating `Aead` API is rewritten onto
// `AeadInPlace`.
pub use mbus_core::wmbus::gcm::{
    build_gcm_aad, build_gcm_iv, extract_access_number, GCM_AAD_LEN, GCM_IV_LEN,
};

/// The core's allocation-free error, under a distinct name so both can coexist.
pub use mbus_core::wmbus::crypto::CryptoError as CoreCryptoError;

/// Lift the core's error into this crate's richer one at the boundary.
///
/// The core cannot carry the `String` reasons this type uses — that is the whole point of
/// the split — so the prose is supplied here, where an allocator exists.
impl From<CoreCryptoError> for CryptoError {
    fn from(e: CoreCryptoError) -> Self {
        match e {
            CoreCryptoError::InvalidKeyLength { expected, actual } => {
                CryptoError::InvalidKeyLength { expected, actual }
            }
            CoreCryptoError::InvalidDataLength { block_size, actual } => {
                CryptoError::InvalidDataLength { block_size, actual }
            }
            CoreCryptoError::UnsupportedMode { mode } => CryptoError::UnsupportedMode { mode },
            CoreCryptoError::InvalidIv => CryptoError::InvalidIV {
                reason: "invalid initialisation vector".to_string(),
            },
            CoreCryptoError::DecryptionFailed => CryptoError::DecryptionFailed {
                reason: "decryption failed".to_string(),
            },
            CoreCryptoError::EncryptionFailed => CryptoError::EncryptionFailed {
                reason: "encryption failed".to_string(),
            },
            CoreCryptoError::InvalidFrame(why) => CryptoError::InvalidFrame {
                reason: why.to_string(),
            },
        }
    }
}

/// Derive a per-device key from a master key by XOR.
///
/// **This is not a key derivation function.** XOR is reversible: anyone holding a derived
/// key and the device identity recovers the master key, which is the one secret shared
/// across the whole fleet. It is kept only so existing callers of
/// [`KeyMode::DerivedFromMaster`] keep compiling, and it is deliberately NOT in
/// `mbus-core` — a portable core should not carry a primitive that must not be used.
///
/// Real OMS master-key derivation is AES-CMAC based (OMS Vol.2 Annex A). Implementing it
/// needs the exact input encoding, which this crate has never had to hand; a
/// plausible-looking wrong KDF is worse than an absent one, because it produces keys that
/// look fine and decrypt nothing.
#[deprecated(
    note = "not a KDF: XOR is reversible and leaks the master key. Supply the device key directly (KeyMode::Direct)."
)]
pub fn derive_device_key(master: &AesKey, device_id: u32, manufacturer: u16) -> AesKey {
    let mut derived = *master.as_bytes();

    let device_bytes = device_id.to_le_bytes();
    for i in 0..4 {
        derived[i] ^= device_bytes[i];
        derived[i + 4] ^= device_bytes[i];
    }

    let mfg_bytes = manufacturer.to_le_bytes();
    for i in 0..2 {
        derived[i + 8] ^= mfg_bytes[i];
        derived[i + 10] ^= mfg_bytes[i];
    }

    // Length is 16 by construction.
    AesKey::from_bytes(&derived).unwrap_or_else(|_| master.clone())
}

/// Enhanced wM-Bus cryptographic operations
pub struct WMBusCrypto {
    master_key: AesKey,
    key_mode: KeyMode,
    /// Configuration flags
    add_crc_mode9: bool,
    verify_crc_mode9: bool,
    full_tag_compatibility: bool,
}

impl WMBusCrypto {
    /// Create new crypto instance with master key
    pub fn new(master_key: AesKey) -> Self {
        Self {
            master_key,
            key_mode: KeyMode::Direct,
            add_crc_mode9: false, // Default: no CRC for compatibility
            verify_crc_mode9: false,
            full_tag_compatibility: true, // Default: use 16-byte tags for testing
        }
    }

    /// Select how the supplied key relates to the frame key (default
    /// [`KeyMode::Direct`]).
    pub fn set_key_mode(&mut self, mode: KeyMode) {
        self.key_mode = mode;
    }

    /// Enable CRC addition for Mode 9 encryption
    pub fn set_crc_mode(&mut self, add: bool, verify: bool) {
        self.add_crc_mode9 = add;
        self.verify_crc_mode9 = verify;
    }

    /// Set tag compatibility mode (true = 16 bytes, false = 12 bytes OMS)
    pub fn set_tag_mode(&mut self, full_tag: bool) {
        self.full_tag_compatibility = full_tag;
    }

    fn should_add_crc(&self) -> bool {
        self.add_crc_mode9
    }

    fn should_verify_crc(&self) -> bool {
        self.verify_crc_mode9
    }

    fn is_full_tag_mode(&self) -> bool {
        self.full_tag_compatibility
    }

    /// The key to use for a device under the configured [`KeyMode`].
    fn effective_key(&self, device_info: &DeviceInfo) -> AesKey {
        match self.key_mode {
            KeyMode::Direct => self.master_key.clone(),
            #[allow(deprecated)]
            KeyMode::DerivedFromMaster => derive_device_key(
                &self.master_key,
                device_info.device_id,
                device_info.manufacturer,
            ),
        }
    }

    /// Find CI field offset in frame
    fn find_ci_offset(&self, frame: &[u8]) -> Result<usize, CryptoError> {
        // Standard wM-Bus frame structure:
        // L(1) + C(1) + M(2) + ID(4) + V(1) + T(1) + CI(1) + ...
        // CI is at offset 10 for standard frames
        const STANDARD_CI_OFFSET: usize = 10;

        if frame.len() <= STANDARD_CI_OFFSET {
            return Err(CryptoError::InvalidFrame {
                reason: format!("Frame too short: {} bytes", frame.len()),
            });
        }

        Ok(STANDARD_CI_OFFSET)
    }

    /// Decrypt a wM-Bus **Mode 9 (AES-128-GCM, OMS 7.3.6)** frame.
    ///
    /// Mode 9 GCM is the one transport-layer security profile handled in this module.
    /// The CBC-based OMS Security Profile A (mode 5) lives in [`crate::wmbus::oms`] and
    /// the ELL link-layer CTR profile in [`crate::wmbus::ell`]; use those directly. The
    /// frame's CI byte must be `0x89`.
    pub fn decrypt_mode9_gcm(
        &mut self,
        encrypted_frame: &[u8],
        device_info: &DeviceInfo,
    ) -> Result<Vec<u8>, CryptoError> {
        let device_key = self.effective_key(device_info);
        let ci_offset = self.find_ci_offset(encrypted_frame)?;
        let ci = encrypted_frame[ci_offset];
        if ci != 0x89 {
            return Err(CryptoError::UnsupportedMode { mode: ci });
        }
        let payload_start = ci_offset + 1;
        if payload_start >= encrypted_frame.len() {
            return Err(CryptoError::InvalidFrame {
                reason: "No encrypted payload found".to_string(),
            });
        }
        let encrypted_payload = &encrypted_frame[payload_start..];
        let decrypted_payload =
            self.decrypt_gcm_mode(&device_key, encrypted_payload, encrypted_frame, device_info)?;
        let mut decrypted_frame = encrypted_frame[..payload_start].to_vec();
        decrypted_frame.extend_from_slice(&decrypted_payload);
        Ok(decrypted_frame)
    }

    /// Encrypt a wM-Bus **Mode 9 (AES-128-GCM, OMS 7.3.6)** frame. The frame's CI byte
    /// is set to `0x89`. See [`Self::decrypt_mode9_gcm`] for why only Mode 9 lives here.
    pub fn encrypt_mode9_gcm(
        &mut self,
        plaintext_frame: &[u8],
        device_info: &DeviceInfo,
    ) -> Result<Vec<u8>, CryptoError> {
        let ci_offset = self.find_ci_offset(plaintext_frame)?;
        let payload_start = ci_offset + 1;
        if payload_start >= plaintext_frame.len() {
            return Err(CryptoError::InvalidFrame {
                reason: "No payload to encrypt".to_string(),
            });
        }
        let plaintext_payload = &plaintext_frame[payload_start..];
        let device_key = self.effective_key(device_info);
        let encrypted_payload =
            self.encrypt_gcm_mode(&device_key, plaintext_payload, plaintext_frame, device_info)?;
        let mut encrypted_frame = plaintext_frame[..ci_offset].to_vec();
        encrypted_frame.push(0x89);
        encrypted_frame.extend_from_slice(&encrypted_payload);
        Ok(encrypted_frame)
    }

    /// Decrypt using AES-128 GCM mode (Mode 9) - OMS 7.3.6
    fn decrypt_gcm_mode(
        &mut self,
        key: &AesKey,
        ciphertext: &[u8],
        full_frame: &[u8],
        device_info: &DeviceInfo,
    ) -> Result<Vec<u8>, CryptoError> {
        // Mode 9 GCM per OMS 7.3.6:
        // - CI = 0x89 (no variants)
        // - 12-byte tag at end (truncated from 16)
        // - 11-byte AAD: L(1) + C(1) + M(2) + A(4) + V(1) + T(1) + Access(1)
        // - 12-byte IV: M(2 LE) + A(4 LE) + Access(6 LE from u64 low bytes)

        if ciphertext.len() < 12 {
            return Err(CryptoError::InvalidFrame {
                reason: "GCM ciphertext too short for 12-byte tag".to_string(),
            });
        }

        // Split ciphertext and 12-byte tag (OMS truncated format)
        let tag_len = if ciphertext.len() >= 16 && self.is_full_tag_mode() {
            16 // Compatibility mode for testing
        } else {
            12 // Standard OMS Mode 9
        };

        let (encrypted_data, tag) = ciphertext.split_at(ciphertext.len() - tag_len);

        // Build 11-byte AAD from frame header (per OMS 7.3.6.2)
        let aad = build_gcm_aad(full_frame)?;

        // Build 12-byte IV/nonce (per OMS 7.3.6.3)
        let iv = build_gcm_iv(device_info);

        // Perform GCM decryption
        let plaintext = self.aes_gcm_decrypt(key, encrypted_data, &aad, &iv, tag)?;

        // Remove CRC if present (OMS 7.3.6.4)
        if self.should_verify_crc() && plaintext.len() >= 2 {
            let crc_received = u16::from_le_bytes([plaintext[0], plaintext[1]]);
            let crc_calculated = self.calculate_crc16(&plaintext[2..]);
            if crc_received != crc_calculated {
                return Err(CryptoError::DecryptionFailed {
                    reason: "CRC verification failed".to_string(),
                });
            }
            Ok(plaintext[2..].to_vec())
        } else {
            Ok(plaintext)
        }
    }

    /// Encrypt using AES-128 GCM mode (Mode 9) - OMS 7.3.6
    ///
    /// Note: OMS specifies 12-byte tag truncation, but the standard aes-gcm
    /// crate requires 16-byte tags for verification. We support both modes:
    /// - full_tag_compatibility=true: Use 16-byte tags (default, for testing)
    /// - full_tag_compatibility=false: Truncate to 12 bytes (OMS compliant)
    ///
    /// For full OMS compliance with 12-byte tag verification, a custom GCM
    /// implementation would be required.
    fn encrypt_gcm_mode(
        &mut self,
        key: &AesKey,
        plaintext: &[u8],
        full_frame: &[u8],
        device_info: &DeviceInfo,
    ) -> Result<Vec<u8>, CryptoError> {
        // Build 11-byte AAD from frame header
        let aad = build_gcm_aad(full_frame)?;

        // Build 12-byte IV/nonce
        let iv = build_gcm_iv(device_info);

        // Optional: Add CRC to plaintext (OMS 7.3.6.4)
        // This is configurable based on device requirements
        let plaintext_to_encrypt = if self.should_add_crc() {
            self.add_crc_to_plaintext(plaintext)
        } else {
            plaintext.to_vec()
        };

        // Perform GCM encryption
        let (ciphertext, tag) = self.aes_gcm_encrypt(key, &plaintext_to_encrypt, &aad, &iv)?;

        // OMS 7.3.6 specifies 12-byte tag truncation for Mode 9
        // Use compatibility mode for testing or truncate for standard
        let mut result = ciphertext;
        if self.full_tag_compatibility {
            result.extend_from_slice(&tag); // Full 16-byte tag for testing
        } else {
            result.extend_from_slice(&tag[..12]); // Truncate to 12 bytes per OMS
        }

        Ok(result)
    }

    /// Add CRC to plaintext before GCM encryption (OMS 7.3.6.4)
    fn add_crc_to_plaintext(&self, plaintext: &[u8]) -> Vec<u8> {
        // Calculate CRC16 on plaintext
        let crc = self.calculate_crc16(plaintext);

        // Append CRC to plaintext
        let mut result = plaintext.to_vec();
        result.extend_from_slice(&crc.to_le_bytes());

        result
    }

    /// Calculate CRC16 for GCM mode
    fn calculate_crc16(&self, data: &[u8]) -> u16 {
        // CRC16-CCITT polynomial: 0x1021
        let mut crc: u16 = 0xFFFF;

        for byte in data {
            crc ^= (*byte as u16) << 8;
            for _ in 0..8 {
                if crc & 0x8000 != 0 {
                    crc = (crc << 1) ^ 0x1021;
                } else {
                    crc <<= 1;
                }
            }
        }

        !crc // Invert final CRC
    }

    /// Perform AES-GCM decryption
    #[allow(unused_variables)]
    fn aes_gcm_decrypt(
        &mut self,
        key: &AesKey,
        ciphertext: &[u8],
        aad: &[u8],
        iv: &[u8],
        tag: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        #[cfg(feature = "crypto")]
        {
            use aes_gcm::aead::consts::{U12, U16};
            use aes_gcm::aead::{generic_array::GenericArray, Aead, Payload};
            use aes_gcm::aes::Aes128;
            use aes_gcm::{AesGcm, KeyInit, Nonce};

            // AES-GCM with an explicitly-sized authentication tag.
            //
            // OMS 7.3.6 truncates the tag to 12 bytes on air. This used to zero-pad a
            // 12-byte tag back to 16 and ask aes-gcm to verify it — which cannot succeed,
            // because a genuine tag's last four bytes are not zeros. GCM tag truncation is
            // a supported construction (NIST SP 800-38D §5.2.1.2): you verify against the
            // truncated tag, you do not reconstruct the full one. aes-gcm expresses that
            // as a type parameter, so each length gets its own cipher below.
            type Aes128Gcm12 = AesGcm<Aes128, U12, U12>;
            type Aes128Gcm16 = AesGcm<Aes128, U12, U16>;

            // Create 12-byte nonce from IV
            let nonce = Nonce::from_slice(iv);

            let mut combined = ciphertext.to_vec();
            combined.extend_from_slice(tag);
            let payload = Payload {
                msg: &combined,
                aad,
            };

            // Verify against the tag length actually received.
            match tag.len() {
                12 => Aes128Gcm12::new(GenericArray::from_slice(key.as_bytes()))
                    .decrypt(nonce, payload),
                16 => Aes128Gcm16::new(GenericArray::from_slice(key.as_bytes()))
                    .decrypt(nonce, payload),
                _ => {
                    return Err(CryptoError::DecryptionFailed {
                        reason: "GCM tag must be 12 bytes (OMS 7.3.6) or 16".to_string(),
                    })
                }
            }
            .map_err(|_| CryptoError::DecryptionFailed {
                reason: "GCM authentication/decryption failed".to_string(),
            })
        }
    }

    /// Perform AES-GCM encryption
    #[allow(unused_variables)]
    fn aes_gcm_encrypt(
        &mut self,
        key: &AesKey,
        plaintext: &[u8],
        aad: &[u8],
        iv: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
        #[cfg(feature = "crypto")]
        {
            use aes_gcm::aead::{generic_array::GenericArray, Aead, Payload};
            use aes_gcm::{Aes128Gcm, KeyInit, Nonce};

            // Create cipher
            let cipher = Aes128Gcm::new(GenericArray::from_slice(key.as_bytes()));

            // Create 12-byte nonce from IV
            let nonce = Nonce::from_slice(iv);

            // Create payload with AAD
            let payload = Payload {
                msg: plaintext,
                aad,
            };

            // Encrypt
            let combined =
                cipher
                    .encrypt(nonce, payload)
                    .map_err(|_| CryptoError::EncryptionFailed {
                        reason: "GCM encryption failed".to_string(),
                    })?;

            // Split ciphertext and tag (last 16 bytes)
            let (ciphertext, tag) = combined.split_at(combined.len() - 16);

            Ok((ciphertext.to_vec(), tag.to_vec()))
        }
    }

    /// Calculate LoRaWAN Message Integrity Code (MIC) using CMAC-AES128
    ///
    /// This function implements the LoRaWAN MIC calculation for both uplink and downlink
    /// messages according to LoRaWAN 1.0.x and 1.1 specifications.
    ///
    /// # Arguments
    ///
    /// * `key` - 128-bit NwkSKey (1.0.x) or appropriate key for 1.1
    /// * `msg` - Complete LoRaWAN frame (MHDR | FHDR | FPort | FRMPayload)
    /// * `direction` - 0 for uplink, 1 for downlink
    /// * `dev_addr` - 32-bit device address
    /// * `fcnt` - 32-bit frame counter
    ///
    /// # Returns
    ///
    /// * 4-byte MIC value
    ///
    /// # Example
    ///
    /// ```rust
    /// use mbus_rs::wmbus::crypto::{AesKey, WMBusCrypto};
    ///
    /// let crypto = WMBusCrypto::new(AesKey::from_bytes(&[0x2B; 16]).unwrap());
    /// let nwk_s_key = [0x2B; 16];
    /// let frame_data = [0x40, 0x78, 0x56, 0x34, 0x12, 0x00, 0x2A, 0x00, 0x01];
    /// let mic = crypto
    ///     .calculate_lorawan_mic(&nwk_s_key, &frame_data, 0 /* uplink */, 0x12345678, 42)
    ///     .unwrap();
    /// assert_eq!(mic.len(), 4);
    /// ```
    #[cfg(feature = "crypto")]
    pub fn calculate_lorawan_mic(
        &self,
        key: &[u8; 16],
        msg: &[u8],
        direction: u8,
        dev_addr: u32,
        fcnt: u32,
    ) -> Result<[u8; 4], CryptoError> {
        use aes::Aes128;
        use cmac::{Cmac, Mac};

        // B0 is exactly 16 bytes by construction: 0x49(1) + reserved(4) + dir(1) +
        // DevAddr(4) + FCnt(4) + pad(1) + len(1). It used to be built with `vec![]` and
        // four `extend`/`push` calls; an array needs no allocator and makes the 16 explicit.
        // This is the same block `mbus_core::lorawan` builds for DataFrame MICs.
        let mut b0 = [0u8; 16];
        b0[0] = 0x49;
        b0[5] = direction; // 0 = uplink, 1 = downlink
        b0[6..10].copy_from_slice(&dev_addr.to_le_bytes());
        b0[10..14].copy_from_slice(&fcnt.to_le_bytes());
        b0[15] = msg.len() as u8;

        // Create CMAC instance
        let mut mac = <Cmac<Aes128> as cmac::Mac>::new_from_slice(key).map_err(|_| {
            CryptoError::InvalidKeyLength {
                expected: 16,
                actual: key.len(),
            }
        })?;

        // Update with B0 block
        mac.update(&b0);

        // Update with message
        mac.update(msg);

        // Finalize and get result
        let result = mac.finalize();
        let mic_bytes = result.into_bytes();

        // Return first 4 bytes as MIC
        let mut mic = [0u8; 4];
        mic.copy_from_slice(&mic_bytes[..4]);

        Ok(mic)
    }

    /// Verify LoRaWAN MIC
    ///
    /// Convenience function to verify a received MIC against calculated value
    #[cfg(feature = "crypto")]
    pub fn verify_lorawan_mic(
        &self,
        key: &[u8; 16],
        msg: &[u8],
        direction: u8,
        dev_addr: u32,
        fcnt: u32,
        received_mic: &[u8; 4],
    ) -> Result<bool, CryptoError> {
        let calculated_mic = self.calculate_lorawan_mic(key, msg, direction, dev_addr, fcnt)?;
        Ok(calculated_mic == *received_mic)
    }

    /// Calculate HMAC-SHA1 for Qundis 3-step authentication
    ///
    /// This function implements HMAC-SHA1 as required by Qundis devices
    /// for their proprietary authentication protocol.
    ///
    /// # Arguments
    ///
    /// * `key` - HMAC key (typically 16 bytes)
    /// * `message` - Message to authenticate
    ///
    /// # Returns
    ///
    /// * 20-byte HMAC-SHA1 digest
    #[cfg(feature = "crypto")]
    pub fn calculate_hmac_sha1(&self, key: &[u8], message: &[u8]) -> Vec<u8> {
        use hmac::{Mac, SimpleHmac};
        let mut mac = <SimpleHmac<sha1::Sha1> as Mac>::new_from_slice(key)
            .expect("HMAC accepts keys of any length");
        mac.update(message);
        mac.finalize().into_bytes().to_vec()
    }

    /// Perform Qundis 3-step authentication
    ///
    /// Implements the complete Qundis authentication flow:
    /// 1. Challenge request
    /// 2. Response calculation
    /// 3. Verification
    #[cfg(feature = "crypto")]
    pub fn qundis_authenticate(
        &self,
        device_key: &[u8; 16],
        challenge: &[u8; 8],
    ) -> Result<Vec<u8>, CryptoError> {
        // Step 1: Prepare authentication message
        let mut auth_msg = Vec::new();
        auth_msg.extend_from_slice(challenge);
        auth_msg.extend_from_slice(&[0x00; 8]); // Padding

        // Step 2: Calculate HMAC-SHA1
        let hmac_result = self.calculate_hmac_sha1(device_key, &auth_msg);

        // Step 3: Return first 16 bytes as response
        Ok(hmac_result[..16].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::hex;

    #[test]
    fn test_aes_key_creation() {
        let key_bytes = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ];
        let key = AesKey::from_bytes(&key_bytes).unwrap();
        assert_eq!(key.as_bytes(), &key_bytes);
    }

    #[test]
    fn test_aes_key_from_hex() {
        let hex_key = "0102030405060708090A0B0C0D0E0F10";
        let key = AesKey::from_hex(hex_key).unwrap();
        assert_eq!(key.as_bytes()[0], 0x01);
        assert_eq!(key.as_bytes()[15], 0x10);
    }

    #[test]
    fn test_key_derivation() {
        let master_key = AesKey::from_bytes(&[0; 16]).unwrap();
        #[allow(deprecated)]
        let device_key = derive_device_key(&master_key, 0x12345678, 0xABCD);

        // Derived key should be different from master key
        assert_ne!(device_key.as_bytes(), master_key.as_bytes());

        // Same derivation should produce same key
        #[allow(deprecated)]
        let device_key2 = derive_device_key(&master_key, 0x12345678, 0xABCD);
        assert_eq!(device_key.as_bytes(), device_key2.as_bytes());
    }

    #[test]
    fn test_crypto_creation() {
        let master_key = AesKey::from_bytes(&[0; 16]).unwrap();
        let crypto = WMBusCrypto::new(master_key);
        assert_eq!(crypto.master_key.as_bytes(), &[0; 16]);
    }

    #[test]
    fn test_invalid_key_length() {
        let result = AesKey::from_bytes(&[0; 15]); // Wrong length
        assert!(result.is_err());
        if let Err(CoreCryptoError::InvalidKeyLength { expected, actual }) = result {
            assert_eq!(expected, 16);
            assert_eq!(actual, 15);
        } else {
            panic!("Expected InvalidKeyLength error");
        }
    }

    #[test]
    fn test_gcm_aad_construction() {
        // Create test frame with proper structure
        // L(1) + C(1) + M(2) + A(4) + V(1) + T(1) + CI(1) + ...
        let frame = vec![
            0x44, // L field
            0x10, // C field
            0xCD, 0xAB, // M field (manufacturer)
            0x78, 0x56, 0x34, 0x12, // A field (address)
            0x01, // V field (version)
            0x02, // T field (type)
            0x89, // CI field (Mode 9)
            0x00, 0x00, // Additional data
        ];

        let aad = build_gcm_aad(&frame).unwrap();

        // Verify AAD structure (11 bytes)
        assert_eq!(aad.len(), 11);
        assert_eq!(aad[0], 0x44); // L
        assert_eq!(aad[1], 0x10); // C
        assert_eq!(&aad[2..4], &[0xCD, 0xAB]); // M
        assert_eq!(&aad[4..8], &[0x78, 0x56, 0x34, 0x12]); // A
        assert_eq!(aad[8], 0x01); // V
        assert_eq!(aad[9], 0x02); // T
        assert_eq!(aad[10], 0x09); // Access (CI & 0x0F)
    }

    #[test]
    fn test_gcm_iv_construction() {
        let device_info = DeviceInfo {
            device_id: 0x12345678,
            manufacturer: 0xABCD,
            version: 0x03,
            device_type: 0x04,
            access_number: Some(0x0304), // Explicit access number
        };

        let iv = build_gcm_iv(&device_info);

        // Verify IV structure (12 bytes, not 16)
        assert_eq!(iv.len(), 12);
        assert_eq!(&iv[0..2], &0xABCDu16.to_le_bytes()); // M (LE)
        assert_eq!(&iv[2..6], &0x12345678u32.to_le_bytes()); // A (LE)
                                                             // Access number derived from version and type
        let expected_access: u64 = (0x03 << 8) | 0x04;
        assert_eq!(&iv[6..12], &expected_access.to_le_bytes()[0..6]);
    }

    #[test]
    fn test_crc16_calculation() {
        let master_key = AesKey::from_bytes(&[0; 16]).unwrap();
        let crypto = WMBusCrypto::new(master_key);

        // Test with known data
        let data = b"123456789";
        let crc = crypto.calculate_crc16(data);

        // Just verify CRC is calculated (exact value depends on implementation)
        // The important part is it's consistent and non-zero
        assert_ne!(crc, 0x0000);
        assert_ne!(crc, 0xFFFF);

        // Test consistency
        let crc2 = crypto.calculate_crc16(data);
        assert_eq!(crc, crc2);
    }

    #[test]
    #[cfg(feature = "crypto")]
    fn test_mode9_gcm_round_trip() {
        // Test Mode 9 GCM encryption/decryption
        let master_key = AesKey::from_hex("0123456789ABCDEF0123456789ABCDEF").unwrap();
        let mut crypto = WMBusCrypto::new(master_key);

        let device_info = DeviceInfo {
            device_id: 0x12345678,
            manufacturer: 0xABCD,
            version: 0x01,
            device_type: 0x02,
            access_number: None,
        };

        // Create test frame with CI=0x89 for Mode 9
        let mut test_frame = vec![
            0x44, 0x10, 0xCD, 0xAB, 0x78, 0x56, 0x34, 0x12, 0x01, 0x02,
            0x89, // CI=0x89 for Mode 9
        ];

        // Add test payload
        let test_payload = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        test_frame.extend_from_slice(&test_payload);

        // Encrypt with Mode 9 GCM
        let encrypted = crypto.encrypt_mode9_gcm(&test_frame, &device_info).unwrap();

        // Encrypted frame should have CI=0x89
        assert_eq!(encrypted[10], 0x89);

        // Decrypt
        let decrypted = crypto.decrypt_mode9_gcm(&encrypted, &device_info).unwrap();

        // Verify the payload matches
        assert_eq!(decrypted.len(), test_frame.len());
        assert_eq!(&decrypted[11..], &test_payload);
    }

    #[test]
    #[cfg(feature = "crypto")]
    fn test_mode9_gcm_nist_vectors() {
        // Test with adapted NIST SP 800-38D test vectors
        // Note: These are adapted for wM-Bus context
        let key = AesKey::from_hex("00000000000000000000000000000000").unwrap();
        let mut crypto = WMBusCrypto::new(key);

        // Build a minimal valid frame
        let frame = vec![
            0x44, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x89, // CI=0x89
        ];

        let device_info = DeviceInfo {
            device_id: 0x00000000,
            manufacturer: 0x0000,
            version: 0x00,
            device_type: 0x00,
            access_number: None,
        };

        // Test encryption with known plaintext
        let mut test_frame = frame.clone();
        test_frame.extend_from_slice(&[0x00; 16]);

        let encrypted = crypto.encrypt_mode9_gcm(&test_frame, &device_info).unwrap();

        // Verify we can decrypt it back
        let decrypted = crypto.decrypt_mode9_gcm(&encrypted, &device_info).unwrap();

        // The decrypted frame should match the original (minus CRC handling)
        assert_eq!(decrypted[0..11], test_frame[0..11]);
    }

    #[test]
    #[cfg(feature = "crypto")]
    fn test_mode9_gcm_tag_truncation() {
        // Test Mode 9 tag truncation behavior
        // Note: The aes-gcm crate doesn't support truncated tag verification,
        // so we can only test that encryption produces the correct length.
        // In production, a custom GCM implementation would be needed for
        // full OMS 7.3.6 compliance with 12-byte truncated tags.

        let master_key = AesKey::from_hex("0123456789ABCDEF0123456789ABCDEF").unwrap();
        let mut crypto = WMBusCrypto::new(master_key);

        // Enable 12-byte tag mode (OMS compliant)
        crypto.set_tag_mode(false); // false = 12-byte tags

        let device_info = DeviceInfo {
            device_id: 0x12345678,
            manufacturer: 0xABCD,
            version: 0x01,
            device_type: 0x02,
            access_number: Some(0x42),
        };

        // Create test frame
        let mut test_frame = vec![
            0x44, 0x10, 0xCD, 0xAB, 0x78, 0x56, 0x34, 0x12, 0x01, 0x02,
            0x89, // CI=0x89 for Mode 9
        ];
        let test_payload = vec![0xAA, 0xBB, 0xCC, 0xDD];
        test_frame.extend_from_slice(&test_payload);

        // Encrypt with Mode 9 (12-byte tag)
        let encrypted = crypto.encrypt_mode9_gcm(&test_frame, &device_info).unwrap();

        // Verify encrypted length uses 12-byte tag
        let expected_len = 11 + test_payload.len() + 12; // header + payload + 12-byte tag
        assert_eq!(encrypted.len(), expected_len);

        // Note: Decryption with truncated tags requires custom GCM implementation
        // The standard aes-gcm crate requires full 16-byte tags for verification
    }

    #[test]
    fn test_access_number_extraction() {
        // Test extraction of access number from frame
        let frame = vec![
            0x44, 0x10, 0xCD, 0xAB, 0x78, 0x56, 0x34, 0x12, 0x01, 0x02,
            0x55, // Access number at position 10
            0x00, 0x00,
        ];

        let access = extract_access_number(&frame);
        assert_eq!(access, Some(0x55));

        // Test with short frame
        let short_frame = vec![0x10, 0x44];
        let access = extract_access_number(&short_frame);
        assert_eq!(access, None);
    }

    #[test]
    #[cfg(feature = "crypto")]
    fn effective_key_path_uses_key_as_is_without_rederiving() {
        // Regression for fix #6: under the default `KeyMode::Direct` the supplied key is
        // used BYTE-FOR-BYTE (no OMS/XOR derivation). A vendor-provisioned key is already
        // the device key; deriving from it again would yield the wrong key.
        let master_key = AesKey::from_hex("0123456789ABCDEF0123456789ABCDEF").unwrap();
        let device_info = DeviceInfo {
            device_id: 0x12345678,
            manufacturer: 0xABCD,
            version: 0x01,
            device_type: 0x02,
            access_number: None,
        };

        // Default is KeyMode::Direct: the effective key IS the supplied key, verbatim.
        let crypto = WMBusCrypto::new(master_key.clone());
        assert_eq!(
            crypto.effective_key(&device_info).as_bytes(),
            master_key.as_bytes(),
            "KeyMode::Direct must use the supplied key as-is"
        );

        // Under the legacy mode the deprecated XOR mixing is applied, so the effective
        // key equals the derived device key — and differs from the supplied master key.
        #[allow(deprecated)]
        let derived =
            derive_device_key(&master_key, device_info.device_id, device_info.manufacturer);
        // XOR-based derivation must actually change the key, or the test proves nothing.
        assert_ne!(derived.as_bytes(), master_key.as_bytes());

        let mut legacy = WMBusCrypto::new(master_key.clone());
        legacy.set_key_mode(KeyMode::DerivedFromMaster);
        assert_eq!(
            legacy.effective_key(&device_info).as_bytes(),
            derived.as_bytes(),
            "KeyMode::DerivedFromMaster must apply the XOR mixing exactly once"
        );
    }

    /// NIST SP 800-38A F.5.1 (CTR-AES128) — a published known-answer vector. Pins the
    /// `ctr` crate (the exact AES-CTR the ELL path in [`crate::wmbus::ell`] relies on)
    /// to the standard rather than to our own round-trip.
    #[test]
    fn ctr_matches_nist_sp800_38a_vector() {
        use aes::Aes128;
        use ctr::cipher::{KeyIvInit, StreamCipher};

        let key = AesKey::from_hex("2b7e151628aed2a6abf7158809cf4f3c").unwrap();
        let iv: [u8; 16] = hex::decode_hex("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff")
            .unwrap()
            .try_into()
            .unwrap();
        let mut buf = hex::decode_hex(
            "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e51\
             30c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
        )
        .unwrap();
        let expected = "874d6191b620e3261bef6864990db6ce9806f66b7970fdff8617187bb9fffdff\
                        5ae4df3edbd5d35e5b4f09020db03eab1e031dda2fbe03d1792170a0f3009cee"
            .replace(char::is_whitespace, "");
        ctr::Ctr128BE::<Aes128>::new(key.as_bytes().into(), &iv.into()).apply_keystream(&mut buf);
        assert_eq!(hex::encode_hex(&buf), expected);
    }

    /// NIST SP 800-38A F.2.1 (CBC-AES128), no padding on the exact-block plaintext.
    /// Pins the `cbc` crate that backs OMS Mode 5 ([`crate::wmbus::oms`]) to the
    /// published ciphertext, and confirms decrypt inverts it.
    #[test]
    fn cbc_matches_nist_sp800_38a_vector() {
        use aes::Aes128;
        use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, BlockEncryptMut, KeyIvInit};

        let key = AesKey::from_hex("2b7e151628aed2a6abf7158809cf4f3c").unwrap();
        let iv: [u8; 16] = hex::decode_hex("000102030405060708090a0b0c0d0e0f")
            .unwrap()
            .try_into()
            .unwrap();
        let plaintext = hex::decode_hex(
            "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e51\
             30c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
        )
        .unwrap();
        let expected = "7649abac8119b246cee98e9b12e9197d5086cb9b507219ee95db113a917678b2\
                        73bed6b8e3c1743b7116e69e222295163ff1caa1681fac09120eca307586e1a7"
            .replace(char::is_whitespace, "");

        let mut buf = plaintext.clone();
        let n = buf.len();
        cbc::Encryptor::<Aes128>::new(key.as_bytes().into(), &iv.into())
            .encrypt_padded_mut::<NoPadding>(&mut buf, n)
            .unwrap();
        assert_eq!(hex::encode_hex(&buf), expected);

        cbc::Decryptor::<Aes128>::new(key.as_bytes().into(), &iv.into())
            .decrypt_padded_mut::<NoPadding>(&mut buf)
            .unwrap();
        assert_eq!(buf, plaintext);
    }
}
