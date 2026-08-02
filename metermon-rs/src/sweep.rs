//! Airwave discovery sweep: mode-agnostic detection of wM-Bus traffic across C, T and
//! S modes, run periodically by the monitor to discover what's on the air.
//!
//! Each sweep does two sync-disabled raw captures ([`capture_raw`]) and analyses them:
//!   - **868.95 MHz / 100 kbps** → C-mode frames (recovered + CRC-checked via the crate's
//!     [`decode_mode_c`](mbus_rs::wmbus::mode_c::decode_mode_c)) and T-mode presence
//!     (3-of-6 line-code detection).
//!   - **868.30 MHz / 32.768 kcps** → S-mode presence (Manchester detection).
//!
//! The analysis functions are pure (no radio) and unit-tested; [`capture_raw`] is the
//! Pi-only raw-SPI capture behind the `radio` feature.

/// The 16 valid 3-of-6 code words (mode T line coding). A run of bytes whose 6-bit
/// groups are all in this set is 3-of-6 encoded (i.e. mode T), not C/S or noise.
const THREE_OF_SIX: [u8; 16] = [
    0x16, 0x0D, 0x0E, 0x0B, 0x1C, 0x19, 0x1A, 0x13, 0x2C, 0x25, 0x26, 0x23, 0x34, 0x31, 0x32, 0x29,
];

/// Normalized C-mode sync word `54 3D 54` as a 24-bit value (searched bit-by-bit).
const C_SYNC24: u32 = 0x54_3D_54;

#[inline]
fn bit(d: &[u8], i: usize) -> u32 {
    ((d[i >> 3] >> (7 - (i & 7))) & 1) as u32
}

/// Read 8 bits starting at an arbitrary bit offset into a byte (MSB-first).
fn byte_at(d: &[u8], bitoff: usize) -> u8 {
    let mut b = 0u8;
    for k in 0..8 {
        b = (b << 1) | bit(d, bitoff + k) as u8;
    }
    b
}

/// Result of analysing the 868.95 MHz capture.
pub struct CtResult {
    /// Recovered, CRC-valid C-mode frames (normalized, type-byte-first).
    pub c_frames: Vec<Vec<u8>>,
    /// Number of post-preamble bursts consistent with 3-of-6 (mode T) encoding.
    pub t_bursts: usize,
}

/// Recover C-mode frames and detect T-mode traffic in a raw 868.95 MHz stream.
pub fn analyze_ct(raw: &[u8]) -> CtResult {
    let nbits = raw.len() * 8;
    let mut c_frames: Vec<Vec<u8>> = Vec::new();
    let mut seen: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();

    // C recovery: rolling 24-bit search for the 54 3D 54 sync, then CRC-validate.
    let mut acc: u32 = 0;
    for i in 0..nbits {
        acc = ((acc << 1) | bit(raw, i)) & 0x00FF_FFFF;
        if i >= 23 && acc == C_SYNC24 {
            let start = i + 1; // frame (type byte) begins just after the sync
            let avail = (nbits - start) / 8;
            if avail < 13 {
                continue;
            }
            let n = avail.min(64);
            let frame: Vec<u8> = (0..n).map(|j| byte_at(raw, start + j * 8)).collect();
            if let Ok(lf) = mbus_rs::wmbus::mode_c::decode_mode_c(&frame) {
                if lf.crc_ok {
                    let plen = mbus_rs::wmbus::radio::rfm69_packet::packet_size(&frame);
                    let f = if plen > 0 && (plen as usize) <= frame.len() {
                        frame[..plen as usize].to_vec()
                    } else {
                        frame
                    };
                    if seen.insert(f.clone()) {
                        c_frames.push(f);
                    }
                }
            }
        }
    }

    CtResult {
        c_frames,
        t_bursts: count_mode_bursts(raw, is_three_of_six),
    }
}

/// Result of analysing the 868.30 MHz capture.
pub struct SResult {
    /// Number of post-preamble bursts consistent with Manchester (mode S) encoding.
    pub s_bursts: usize,
}

/// Detect S-mode (Manchester) traffic in a raw 868.30 MHz stream.
pub fn analyze_s(raw: &[u8]) -> SResult {
    SResult {
        s_bursts: count_mode_bursts(raw, is_manchester),
    }
}

/// Find alternating-bit preamble runs (>=16 bits) and, at each run's end, score the
/// following burst with `scorer`; count bursts scoring >= 0.90.
fn count_mode_bursts(raw: &[u8], scorer: fn(&[u8], usize) -> f64) -> usize {
    let nbits = raw.len() * 8;
    if nbits < 400 {
        return 0;
    }
    let mut count = 0;
    let mut run = 1usize;
    for i in 1..nbits {
        if bit(raw, i) != bit(raw, i - 1) {
            run += 1;
        } else {
            if run >= 16 && scorer(raw, i) >= 0.90 {
                count += 1;
            }
            run = 1;
        }
    }
    count
}

/// Fraction of 6-bit groups (best over 6 bit-alignments x 2 bit-orders) that are valid
/// 3-of-6 code words, over the ~360 bits following `start`.
fn is_three_of_six(raw: &[u8], start: usize) -> f64 {
    let nbits = raw.len() * 8;
    let span = 360.min(nbits.saturating_sub(start));
    if span < 48 {
        return 0.0;
    }
    let mut best = 0.0f64;
    for rev in [false, true] {
        for off in 0..6 {
            let base = start + off;
            let ncw = (span - off) / 6;
            if ncw < 8 {
                continue;
            }
            let mut valid = 0;
            for k in 0..ncw {
                let mut cw = 0u8;
                for b in 0..6 {
                    let bp = base + k * 6 + b;
                    let bitv = bit(raw, bp) as u8;
                    cw = if rev {
                        cw | (bitv << b)
                    } else {
                        (cw << 1) | bitv
                    };
                }
                if THREE_OF_SIX.contains(&cw) {
                    valid += 1;
                }
            }
            best = best.max(valid as f64 / ncw as f64);
        }
    }
    best
}

/// Fraction of 2-chip groups (best over 2 alignments) that are valid Manchester (01/10)
/// AND a mix of both — real S-mode data, not a pure alternating preamble.
fn is_manchester(raw: &[u8], start: usize) -> f64 {
    let nbits = raw.len() * 8;
    let span = 400.min(nbits.saturating_sub(start));
    if span < 64 {
        return 0.0;
    }
    let mut best = 0.0f64;
    for off in [0usize, 1] {
        let base = start + off;
        let np = (span - off) / 2;
        if np < 32 {
            continue;
        }
        let (mut valid, mut c01, mut c10) = (0, 0, 0);
        for k in 0..np {
            let a = bit(raw, base + k * 2);
            let b = bit(raw, base + k * 2 + 1);
            match (a, b) {
                (0, 1) => {
                    valid += 1;
                    c01 += 1;
                }
                (1, 0) => {
                    valid += 1;
                    c10 += 1;
                }
                _ => {}
            }
        }
        let frac = valid as f64 / np as f64;
        // Require a genuine mix so a pure 0101 preamble doesn't score as data.
        if c01 >= np / 5 && c10 >= np / 5 {
            best = best.max(frac);
        }
    }
    best
}

/// Pi-only: sync-disabled raw capture at `freq_hz`/`bitrate_bps` for `seconds`,
/// returning the demodulated byte stream. Mirrors the `probe-raw` subcommand.
#[cfg(feature = "radio")]
pub fn capture_raw(
    spidev: &str,
    freq_hz: u64,
    bitrate_bps: u32,
    seconds: u64,
) -> anyhow::Result<Vec<u8>> {
    use rppal::spi::{BitOrder, Bus, Mode, SlaveSelect, Spi};
    use std::time::{Duration, Instant};

    let tail = spidev
        .rsplit_once("spidev")
        .map(|(_, t)| t)
        .unwrap_or("0.0");
    let (b, c) = tail.split_once('.').unwrap_or(("0", "0"));
    let bus = match b.trim().parse::<u8>().unwrap_or(0) {
        1 => Bus::Spi1,
        2 => Bus::Spi2,
        _ => Bus::Spi0,
    };
    let ss = match c.trim().parse::<u8>().unwrap_or(0) {
        1 => SlaveSelect::Ss1,
        2 => SlaveSelect::Ss2,
        _ => SlaveSelect::Ss0,
    };
    let mut spi = Spi::new(bus, ss, 4_000_000, Mode::Mode0)?;
    spi.set_bit_order(BitOrder::MsbFirst)?;
    let w = |spi: &mut Spi, reg: u8, val: u8| -> anyhow::Result<()> {
        spi.write(&[reg | 0x80, val])?;
        Ok(())
    };
    let r = |spi: &mut Spi, reg: u8| -> anyhow::Result<u8> {
        let mut rx = [0u8; 2];
        spi.transfer(&mut rx, &[reg & 0x7F, 0x00])?;
        Ok(rx[1])
    };

    let frf = ((freq_hz as u128 * 524_288) / 32_000_000) as u32;
    let br = (32_000_000u32 / bitrate_bps.max(1)).max(1);
    w(&mut spi, 0x01, 0x04)?; // STANDBY
    w(&mut spi, 0x02, 0x00)?; // FSK packet, no shaping
    w(&mut spi, 0x03, (br >> 8) as u8)?;
    w(&mut spi, 0x04, br as u8)?;
    w(&mut spi, 0x05, 0x03)?; // FDEV ±50 kHz
    w(&mut spi, 0x06, 0x33)?;
    w(&mut spi, 0x07, (frf >> 16) as u8)?;
    w(&mut spi, 0x08, (frf >> 8) as u8)?;
    w(&mut spi, 0x09, frf as u8)?;
    w(&mut spi, 0x18, 0x88)?; // LNA 200Ω max gain
    w(&mut spi, 0x19, 0xE0)?; // RXBW wide
    w(&mut spi, 0x1E, 0x00)?; // AFC off
    w(&mut spi, 0x2E, 0x40)?; // SyncOff + FifoFillCondition=1 (continuous fill)
    w(&mut spi, 0x37, 0x00)?; // fixed len, no crc/addr
    w(&mut spi, 0x38, 0x00)?; // unlimited length
    w(&mut spi, 0x3C, 0x1F)?; // FIFOTHRESH 31 -> burst 32
    w(&mut spi, 0x3D, 0x00)?;
    w(&mut spi, 0x58, 0x2D)?; // high-sensitivity boost
    w(&mut spi, 0x6F, 0x30)?; // DAGC
    w(&mut spi, 0x01, 0x10)?; // RX

    let mut out = Vec::with_capacity((bitrate_bps as usize / 8) * seconds as usize + 4096);
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        let irq2 = r(&mut spi, 0x28)?;
        if irq2 & 0x20 != 0 {
            let mut rx = [0u8; 33];
            spi.transfer(&mut rx, &[0u8; 33])?;
            out.extend_from_slice(&rx[1..]);
        } else if irq2 & 0x40 != 0 {
            let mut rx = [0u8; 2];
            spi.transfer(&mut rx, &[0x00, 0x00])?;
            out.push(rx[1]);
        }
    }
    w(&mut spi, 0x01, 0x04)?; // STANDBY
    Ok(out)
}

/// Run one full airwave sweep: capture + analyse the C/T band (868.95 MHz) and the
/// S band (868.30 MHz), returning (distinct C meter ids, T-burst count, S-burst count).
#[cfg(feature = "radio")]
pub fn sweep_once(spidev: &str, seconds: u64) -> anyhow::Result<(Vec<u32>, usize, usize)> {
    let raw_ct = capture_raw(spidev, 868_950_000, 100_000, seconds)?;
    let ct = analyze_ct(&raw_ct);
    let raw_s = capture_raw(spidev, 868_300_000, 32_768, seconds)?;
    let s = analyze_s(&raw_s);
    let mut meters: Vec<u32> = ct
        .c_frames
        .iter()
        .filter_map(|f| mbus_rs::wmbus::mode_c::decode_mode_c(f).ok())
        .map(|lf| lf.device_address)
        .collect();
    meters.sort_unstable();
    meters.dedup();
    Ok((meters, ct.t_bursts, s.s_bursts))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pack a bit sequence (MSB-first) into bytes, with `pad` leading filler bits so the
    /// content lands at a non-byte-aligned offset (as a real capture would).
    fn pack(bits: &[u8], pad: usize) -> Vec<u8> {
        let mut all = vec![0u8; pad]; // pad filler bits (0)
        all.extend_from_slice(bits);
        while all.len() % 8 != 0 {
            all.push(0);
        }
        let mut out = vec![0u8; all.len() / 8];
        for (i, &b) in all.iter().enumerate() {
            if b != 0 {
                out[i >> 3] |= 1 << (7 - (i & 7));
            }
        }
        out
    }

    fn bits_of(bytes: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        for &b in bytes {
            for k in (0..8).rev() {
                v.push((b >> k) & 1);
            }
        }
        v
    }

    #[test]
    fn recovers_real_c_frame_from_bitstream() {
        // Real Type B frame (KAM 74644444), prefixed with the 54 3D 54 sync, embedded at
        // a non-byte-aligned offset amid filler — as the sync-off capture would deliver.
        let frame = hex::decode(
            "3d25442d2c444464741b168d208d3048a121f6597959d56873b609a439b99d58531a8a726d9f0c",
        )
        .unwrap();
        let mut payload = bits_of(&[0x54, 0x3D, 0x54]);
        payload.extend(bits_of(&frame));
        let raw = pack(&payload, 3); // 3-bit misalignment
        let res = analyze_ct(&raw);
        assert_eq!(res.c_frames.len(), 1, "should recover the C frame");
        let lf = mbus_rs::wmbus::mode_c::decode_mode_c(&res.c_frames[0]).unwrap();
        assert_eq!(lf.device_address, 74_644_444);
        assert_eq!(res.t_bursts, 0, "a C frame is not 3-of-6");
    }

    #[test]
    fn detects_synthetic_t_mode_3of6() {
        // preamble (alternating) + a long run of valid 3-of-6 code words.
        let mut bits = Vec::new();
        for _ in 0..40 {
            bits.push(0);
            bits.push(1);
        } // 80-bit preamble
        for i in 0..120 {
            let cw = THREE_OF_SIX[i % 16];
            for b in (0..6).rev() {
                bits.push((cw >> b) & 1);
            }
        }
        let raw = pack(&bits, 1);
        assert!(count_mode_bursts(&raw, is_three_of_six) >= 1, "T detected");
        // The same stream is not Manchester data.
        assert_eq!(count_mode_bursts(&raw, is_manchester), 0);
    }

    #[test]
    fn detects_synthetic_s_mode_manchester() {
        // preamble + Manchester-encoded data bits (each data bit -> 01 or 10).
        let mut bits = Vec::new();
        for _ in 0..40 {
            bits.push(0);
            bits.push(1);
        }
        let data = [1u8, 0, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 0, 1, 0, 1];
        for _ in 0..12 {
            for &d in &data {
                if d == 1 {
                    bits.push(1);
                    bits.push(0);
                } else {
                    bits.push(0);
                    bits.push(1);
                }
            }
        }
        let raw = pack(&bits, 2);
        assert!(count_mode_bursts(&raw, is_manchester) >= 1, "S detected");
    }

    #[test]
    fn quiet_noise_detects_nothing() {
        // Deterministic pseudo-random bytes (no Date/rand): an LCG.
        let mut x: u32 = 0x1234_5678;
        let mut b = Vec::new();
        for _ in 0..4000 {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            b.push((x >> 24) as u8);
        }
        let ct = analyze_ct(&b);
        assert_eq!(ct.c_frames.len(), 0);
        assert_eq!(ct.t_bursts, 0);
        assert_eq!(analyze_s(&b).s_bursts, 0);
    }
}
