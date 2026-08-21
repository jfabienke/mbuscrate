//! Cause attribution: deciding whether poor reception is actually an RF problem.
//!
//! Every candidate input to an adaptive radio loop misled us at least once, in the same
//! direction — "the link is bad, turn something up":
//!
//! * **RSSI reported −104 dBm for a device at −74 dBm.** A gain loop keyed on RSSI would
//!   have cranked gain against a fabricated fault. SNR was right throughout, which is why
//!   SNR-vs-RSSI disagreement is the lie detector here.
//! * **Two Qundis meters read 65–68 % CRC at −34 dBm**, where every comparable-strength
//!   meter reads ~100 %. That is not a link problem: the device declares `L = 73` and stops
//!   transmitting after ~50 bytes, so the later block CRCs fail. Adapting RF to it would be
//!   optimising against a protocol quirk.
//! * **A downlink burst makes meters look like they went silent** — see
//!   [`super::blanking`], which is the reason that module exists.
//!
//! The rule this module encodes: **never act on a symptom until the alternatives are
//! excluded.** [`attribute`] returns the most specific non-RF explanation it can find, and
//! only says [`Cause::LikelyRf`] when none applies. Nothing here changes radio settings —
//! deliberately. It exists so that if we ever do adapt, the trigger is a cause and not a
//! symptom.

/// What is most plausibly behind a poor-reception symptom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// Our own transmitter was active — not a reception problem at all.
    SelfBlanked,
    /// A known per-manufacturer protocol quirk, not a link quality issue.
    VendorQuirk,
    /// RSSI and SNR disagree; the RSSI figure is not trustworthy on its own.
    InstrumentSuspect,
    /// Signal is genuinely at the edge of sensitivity — the expected shape of a good
    /// receiver, not a fault.
    EdgeOfSensitivity,
    /// Nothing else explains it. Only here is an RF response even arguable.
    LikelyRf,
}

impl Cause {
    /// Whether an adaptive response would be defensible. False for every non-RF cause —
    /// which is the entire point.
    pub fn warrants_rf_action(self) -> bool {
        matches!(self, Cause::LikelyRf)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Cause::SelfBlanked => "self-blanked",
            Cause::VendorQuirk => "vendor-quirk",
            Cause::InstrumentSuspect => "instrument-suspect",
            Cause::EdgeOfSensitivity => "edge-of-sensitivity",
            Cause::LikelyRf => "likely-rf",
        }
    }
}

/// One reception symptom, with the context needed to explain it.
#[derive(Debug, Clone, Copy)]
pub struct Symptom {
    /// Reported RSSI, dBm.
    pub rssi_dbm: f32,
    /// Reported SNR, dB.
    ///
    /// **`None` for every wM-Bus frame.** The SX126x provides no SNR in GFSK, and no
    /// frequency-error indicator either (0x076B belongs to the LoRa modem). So the
    /// RSSI-vs-SNR cross-check below is available for LoRa only — which is where the
    /// RSSI bug that motivated it actually occurred. For meters we have amplitude and
    /// nothing to check it against, and [`attribute`] must not pretend otherwise.
    pub snr_db: Option<f32>,
    /// Did our own transmitter overlap this frame? From [`super::blanking::Blanking`].
    pub blanked: bool,
    /// Manufacturer code, if the frame decoded far enough to know.
    pub manufacturer: Option<[u8; 3]>,
    /// Observed CRC pass rate for this device, 0.0–1.0.
    pub crc_pass_rate: f32,
}

/// Manufacturers with a known frame-level quirk that depresses CRC pass rates
/// independently of signal quality.
///
/// `QDS` (Qundis): declares `L = 73` on mode-C type-A frames but stops transmitting after
/// ~50 bytes; blocks 1–3 verify, block 4 is zero-filled with CRC `fffe`, block 5 is noise.
/// Confirmed against 21 h of fleet statistics — two units at −34/−38 dBm sitting at 65–68 %
/// CRC while every other meter in the same band reads ~100 % — and independently by a second
/// decoder implementation on the same air.
const QUIRK_MANUFACTURERS: &[[u8; 3]] = &[*b"QDS"];

/// Below this, a depressed CRC rate is the normal behaviour of a receiver working at its
/// limit rather than evidence of a fault. Our own fleet runs 62–66 % CRC in the −110/−120
/// bands with no problem to fix.
const EDGE_OF_SENSITIVITY_DBM: f32 = -100.0;

/// A LoRa/GFSK link with healthy SNR cannot simultaneously have a very weak RSSI; when the
/// two disagree by more than this, the RSSI reading is the suspect one.
const RSSI_SNR_DISAGREEMENT_DB: f32 = 15.0;

/// Attribute a symptom to its most specific non-RF cause.
///
/// Ordering matters and is deliberate: blanking first (we caused it), then vendor quirks
/// (not a link at all), then instrument distrust (the number is wrong), then the edge of
/// sensitivity (working as designed). `LikelyRf` is the residual.
pub fn attribute(s: &Symptom) -> Cause {
    if s.blanked {
        return Cause::SelfBlanked;
    }
    if let Some(m) = s.manufacturer {
        if QUIRK_MANUFACTURERS.contains(&m) {
            return Cause::VendorQuirk;
        }
    }
    // A strong SNR alongside a very weak RSSI is the signature of the RSSI bug: the link is
    // demonstrably good, so the amplitude figure is not to be trusted. LoRa only — GFSK
    // reports no SNR, so for meters this check is simply unavailable rather than passing.
    if let Some(snr) = s.snr_db {
        if snr > 0.0
            && s.rssi_dbm < EDGE_OF_SENSITIVITY_DBM
            && snr > RSSI_SNR_DISAGREEMENT_DB - 15.0
        {
            return Cause::InstrumentSuspect;
        }
    }
    if s.rssi_dbm < EDGE_OF_SENSITIVITY_DBM {
        return Cause::EdgeOfSensitivity;
    }
    Cause::LikelyRf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(rssi: f32, snr: Option<f32>, crc: f32) -> Symptom {
        Symptom {
            rssi_dbm: rssi,
            snr_db: snr,
            blanked: false,
            manufacturer: None,
            crc_pass_rate: crc,
        }
    }

    #[test]
    fn our_own_transmission_outranks_every_other_explanation() {
        let mut s = sym(-105.0, Some(-5.0), 0.1);
        s.blanked = true;
        s.manufacturer = Some(*b"QDS");
        assert_eq!(attribute(&s), Cause::SelfBlanked);
        assert!(!attribute(&s).warrants_rf_action());
    }

    #[test]
    fn the_qundis_quirk_is_not_an_rf_problem() {
        // The real case: 65-68% CRC at -34 dBm. Without this rule an adaptive loop would
        // chase a phantom link fault on the strongest meters in the fleet.
        let mut s = sym(-34.0, Some(10.0), 0.66);
        s.manufacturer = Some(*b"QDS");
        assert_eq!(attribute(&s), Cause::VendorQuirk);
        assert!(!attribute(&s).warrants_rf_action());
    }

    #[test]
    fn a_healthy_meter_at_the_same_strength_is_judged_on_its_merits() {
        // Same RSSI, different manufacturer: the quirk rule must not launder real faults.
        let mut s = sym(-34.0, Some(10.0), 0.66);
        s.manufacturer = Some(*b"KAM");
        assert_eq!(attribute(&s), Cause::LikelyRf);
        assert!(attribute(&s).warrants_rf_action());
    }

    #[test]
    fn strong_snr_with_implausible_rssi_discredits_the_rssi() {
        // The bug that cost a day: -104 dBm reported for a device actually at -74, with
        // SNR healthy throughout. SNR is the tie-breaker.
        let s = sym(-104.0, Some(5.0), 1.0);
        assert_eq!(attribute(&s), Cause::InstrumentSuspect);
        assert!(!attribute(&s).warrants_rf_action());
    }

    #[test]
    fn a_genuinely_weak_signal_with_matching_snr_is_edge_of_sensitivity() {
        // -105 dBm with negative SNR is consistent, and is what a good receiver looks like
        // at its limit. Our fleet runs 62-66% CRC in these bands with nothing to fix.
        let s = sym(-105.0, Some(-8.0), 0.64);
        assert_eq!(attribute(&s), Cause::EdgeOfSensitivity);
        assert!(!attribute(&s).warrants_rf_action());
    }

    #[test]
    fn a_strong_signal_failing_crc_with_no_other_explanation_is_the_only_rf_case() {
        let s = sym(-60.0, Some(9.0), 0.30);
        assert_eq!(attribute(&s), Cause::LikelyRf);
        assert!(attribute(&s).warrants_rf_action());
    }

    #[test]
    fn wmbus_frames_have_no_snr_so_the_instrument_check_is_unavailable() {
        // The SX126x reports no SNR in GFSK. Absence must degrade to the weaker
        // edge-of-sensitivity judgement, never be read as "SNR agrees".
        let s = sym(-105.0, None, 0.5);
        assert_eq!(attribute(&s), Cause::EdgeOfSensitivity);
        // And a strong meter with no SNR still gets judged on its merits.
        let s2 = sym(-60.0, None, 0.30);
        assert_eq!(attribute(&s2), Cause::LikelyRf);
    }

    #[test]
    fn the_rssi_bug_would_be_caught_on_lora_but_not_on_wmbus() {
        // Documents a real limitation rather than hiding it: the -104-for--74 bug was a
        // LoRa frame and IS caught; the same amplitude error on a meter is indistinguishable
        // from a genuinely weak signal, because GFSK gives us nothing to cross-check.
        let lora = sym(-104.0, Some(5.0), 1.0);
        assert_eq!(attribute(&lora), Cause::InstrumentSuspect);
        let wmbus = sym(-104.0, None, 1.0);
        assert_eq!(attribute(&wmbus), Cause::EdgeOfSensitivity);
    }

    #[test]
    fn every_non_rf_cause_refuses_to_warrant_action() {
        for c in [
            Cause::SelfBlanked,
            Cause::VendorQuirk,
            Cause::InstrumentSuspect,
            Cause::EdgeOfSensitivity,
        ] {
            assert!(
                !c.warrants_rf_action(),
                "{} must not warrant action",
                c.as_str()
            );
        }
        assert!(Cause::LikelyRf.warrants_rf_action());
    }
}
