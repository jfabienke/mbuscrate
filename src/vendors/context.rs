//! Per-frame decode context — the single seam through which vendor behaviour reaches
//! the decoder (`docs/design/vendor-layers.md` §4.1, migration step 1).
//!
//! Three of the design's principles are enforced *structurally* here rather than by
//! convention:
//!
//! - **P6** — the vendor binding is resolved only when the identity-bearing link
//!   header passed its own integrity check. A corrupt manufacturer code can never
//!   select vendor code, because the binding simply does not exist on such a frame.
//! - **P7** — there is one decode path. This context is what the retired
//!   `*_with_vendor` forks threaded as loose `(Option<&str>, Option<&Registry>)`
//!   pairs, resolved differently at every call site.
//! - **P9** — the binding is resolved once per frame, at construction, not per field.

use crate::vendors::{VendorExtension, VendorRegistry};
use std::sync::Arc;

/// Identity of the transmitting device, decoded from the frame's link header.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceIdentity {
    /// Three-letter manufacturer code decoded from the M-field (e.g. `"KAM"`).
    pub manufacturer: String,
    /// Device version byte from the A-field.
    pub version: u8,
    /// Device type / medium byte from the A-field.
    pub device_type: u8,
    /// Meter address (BCD-decoded id).
    pub address: u32,
    /// Resolved product/firmware profile supplied by the Device Manager
    /// (design §1.1 case 3), when the gateway holds one for this device. Selects
    /// model-specific interpretation (e.g. which status-bit table applies).
    /// `None` is the normal case and decoding then falls back to raw for anything
    /// the frame alone cannot disambiguate.
    pub profile: Option<DeviceProfile>,
}

/// A device profile resolved out of band by the backend Device Manager. The crate
/// consumes this as an input; it never ships or guesses one (design §1.1).
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceProfile {
    /// Product model, e.g. `"MULTICAL 21"`.
    pub model: String,
    /// Firmware or configuration revision, where the catalog knows it.
    pub firmware: Option<String>,
}

/// Per-region frame integrity (P6).
///
/// Integrity is deliberately not one boolean: the identity-bearing header gates all
/// vendor dispatch, while later payload blocks are what a `tolerate_crc` quirk may
/// legitimately relax.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Integrity {
    /// The link-header region carrying the M/A fields validated.
    pub header_valid: bool,
    /// Every payload block validated. `false` with `header_valid: true` means the
    /// identity is trustworthy but some content is not.
    pub payload_valid: bool,
}

impl Integrity {
    /// Integrity for a frame whose every check passed.
    pub fn valid() -> Self {
        Self {
            header_valid: true,
            payload_valid: true,
        }
    }
}

/// The per-frame decode context: device identity, per-region integrity, and the
/// vendor binding resolved from them.
///
/// An empty context (no vendor) is the normal case and costs one branch per hook
/// site.
#[derive(Clone, Default)]
pub struct DecodeContext {
    /// Identity from the link header — the vendor dispatch key.
    pub device: DeviceIdentity,
    /// Per-region integrity; consulted at binding resolution (P6).
    pub integrity: Integrity,
    /// Extension resolved for this frame, if any. Private: hook sites go through
    /// [`extension`](Self::extension) so the P6 gate cannot be bypassed.
    extension: Option<Arc<dyn VendorExtension>>,
}

impl DecodeContext {
    /// A context with no vendor binding and unknown identity. Decoding under it is
    /// pure Layer 0.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Resolve the vendor binding for a frame.
    ///
    /// The binding is looked up once, here (P9), and only when the identity-bearing
    /// header validated (P6): with `integrity.header_valid == false` the returned
    /// context carries device and integrity for reporting, but no vendor code will
    /// ever run for the frame.
    pub fn resolve(
        registry: Option<&VendorRegistry>,
        device: DeviceIdentity,
        integrity: Integrity,
    ) -> Self {
        let extension = if integrity.header_valid {
            registry.and_then(|r| r.get(&device.manufacturer))
        } else {
            None
        };
        Self {
            device,
            integrity,
            extension,
        }
    }

    /// Compatibility constructor for the deprecated `*_with_vendor` entry points,
    /// which predate integrity tracking and therefore assumed a valid frame.
    pub(crate) fn assume_valid(manufacturer: &str, registry: Option<&VendorRegistry>) -> Self {
        Self::resolve(
            registry,
            DeviceIdentity {
                manufacturer: manufacturer.to_string(),
                ..DeviceIdentity::default()
            },
            Integrity::valid(),
        )
    }

    /// The extension bound to this frame, if the registry had one for the
    /// manufacturer *and* the header validated.
    pub fn extension(&self) -> Option<&Arc<dyn VendorExtension>> {
        self.extension.as_ref()
    }

    /// The manufacturer code this context dispatches under.
    pub fn manufacturer(&self) -> &str {
        &self.device.manufacturer
    }
}

impl std::fmt::Debug for DecodeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecodeContext")
            .field("device", &self.device)
            .field("integrity", &self.integrity)
            .field("has_extension", &self.extension.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qds_identity() -> DeviceIdentity {
        DeviceIdentity {
            manufacturer: "QDS".into(),
            version: 1,
            device_type: 0x08,
            address: 12345678,
            profile: None,
        }
    }

    #[test]
    fn corrupt_header_never_binds_a_vendor() {
        let registry = VendorRegistry::with_defaults().unwrap();
        let ctx = DecodeContext::resolve(
            Some(&registry),
            qds_identity(),
            Integrity {
                header_valid: false,
                payload_valid: false,
            },
        );
        assert!(
            ctx.extension().is_none(),
            "P6: a corrupt manufacturer code must not select vendor code"
        );
    }

    #[test]
    fn valid_header_binds_the_registered_vendor() {
        let registry = VendorRegistry::with_defaults().unwrap();
        let ctx = DecodeContext::resolve(Some(&registry), qds_identity(), Integrity::valid());
        assert!(ctx.extension().is_some());
        assert_eq!(ctx.manufacturer(), "QDS");
    }

    #[test]
    fn empty_context_is_pure_layer0() {
        let ctx = DecodeContext::empty();
        assert!(ctx.extension().is_none());
        assert_eq!(ctx.manufacturer(), "");
    }
}
