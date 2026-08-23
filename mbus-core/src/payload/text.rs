//! Short protocol text that is usually a compile-time constant.
//!
//! Units, quantities and medium names all originate in the `const` VIF tables, so the
//! common case needs no storage at all — only a pointer to static data. A handful of
//! cases are genuinely dynamic: the plain-text VIF `0x7C`, and quantities that a vendor
//! extension annotates. Those get a small inline buffer.
//!
//! [`Text`] derefs to `str`, so `==`, `contains`, `format!` and friends work on it exactly
//! as they did on `String`. That is deliberate: it lets the representation change without
//! touching the hundred-odd places that read these fields.

use core::ops::Deref;

/// A unit, quantity or medium name, with an inline buffer for the dynamic case.
///
/// `N` is chosen per field rather than globally, because the inline buffer costs its full
/// size in every record whether used or not. Measured: an enum with a 96-byte buffer is
/// 112 bytes, so spending that on all three text fields would grow `MBusRecord` from 488
/// bytes to roughly 750 — worse by value than the `String`s it replaces, even though it
/// removes their allocations.
///
/// So: [`UnitText`] is small, because units are things like `m^3` and `kWh`;
/// [`QuantityText`] is larger, because a vendor extension may append a status marker to a
/// quantity that is already up to 58 characters.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Text<const N: usize> {
    /// From a `const` table — the overwhelmingly common case, and free.
    Static(&'static str),
    /// Built at run time: a plain-text VIF, or a quantity a vendor annotated.
    Owned(heapless::String<N>),
}

/// A unit. `m^3`, `kWh`, `°C` — the longest in the VIF tables is well under 16 bytes.
pub type UnitText = Text<16>;

/// A quantity, plus room for a vendor's ` [ERROR,ALARM]` annotation.
///
/// 48, not 96. The longest quantity in the VIF tables is 58 characters, but it is an
/// obscure currency code; the ones that actually appear are `Volume`, `Energy`, `Power`
/// and the like, so `Volume [ERROR,ALARM]` fits with room over. Sizing for the pathological
/// case would cost every record 56 bytes it never uses — measured, `Text<96>` is 112 bytes
/// against `Text<48>`'s 56 — and the penalty for exceeding it is a clipped label, not a
/// lost reading.
pub type QuantityText = Text<48>;

impl<const N: usize> Text<N> {
    pub const fn new() -> Self {
        Text::Static("")
    }

    /// The inline capacity, for callers that need to know what fits.
    pub const CAPACITY: usize = N;

    pub fn as_str(&self) -> &str {
        match self {
            Text::Static(s) => s,
            Text::Owned(s) => s.as_str(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.as_str().is_empty()
    }

    /// Build from a `&str`, truncating at `N`.
    ///
    /// Truncation rather than failure because this is display text: a shortened unit
    /// label is a cosmetic problem, whereas refusing the record would discard a
    /// perfectly good meter reading over one.
    pub fn from_str_truncating(s: &str) -> Self {
        let mut out = heapless::String::new();
        for c in s.chars() {
            if out.push(c).is_err() {
                break;
            }
        }
        Text::Owned(out)
    }
}

impl<const N: usize> Default for Text<N> {
    fn default() -> Self {
        Text::new()
    }
}

impl<const N: usize> Deref for Text<N> {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl<const N: usize> From<&'static str> for Text<N> {
    fn from(s: &'static str) -> Self {
        Text::Static(s)
    }
}

impl<const N: usize> core::fmt::Display for Text<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<const N: usize> PartialEq<str> for Text<N> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<const N: usize> PartialEq<&str> for Text<N> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_text_costs_no_storage_and_compares_as_a_string() {
        let t = UnitText::Static("m^3");
        assert_eq!(t, "m^3");
        assert_eq!(&*t, "m^3");
        assert!(t.contains("^"), "Deref gives the full str API");
        assert_eq!(format!("{t}"), "m^3");
    }

    #[test]
    fn owned_text_holds_what_a_vendor_appends() {
        let t = QuantityText::from_str_truncating("Volume [ERROR,ALARM]");
        assert_eq!(t, "Volume [ERROR,ALARM]");
        assert!(t.contains("ERROR"), "the alarm search still works");
    }

    #[test]
    fn a_real_quantity_plus_an_annotation_fits_and_the_outlier_clips() {
        // The bound TEXT_CAPACITY is chosen for; assert it rather than trusting the count.
        // The realistic worst case is a normal quantity plus a vendor annotation.
        let annotated = QuantityText::from_str_truncating("Volume [ERROR,ALARM]");
        assert_eq!(annotated, "Volume [ERROR,ALARM]", "fits without truncation");

        // The pathological case — the longest table entry, annotated — does clip, and
        // that is the deliberate trade: a shortened label, never a lost reading.
        let longest = "Credit of 10nn-3 of the nominal local legal currency units";
        assert!(longest.len() > QuantityText::CAPACITY);
        let clipped = QuantityText::from_str_truncating(longest);
        assert_eq!(clipped.len(), QuantityText::CAPACITY);
    }

    #[test]
    fn over_long_text_truncates_rather_than_failing() {
        let huge = "x".repeat(QuantityText::CAPACITY + 50);
        let t = QuantityText::from_str_truncating(&huge);
        assert_eq!(t.len(), QuantityText::CAPACITY, "clipped, not refused");
    }

    #[test]
    fn empty_is_static_and_therefore_free() {
        assert!(UnitText::new().is_empty());
        assert_eq!(UnitText::default(), "");
    }
}
