//! Finding a wM-Bus frame's boundary in a byte stream.
//!
//! Where [`super::mode_c`] decodes what is *inside* a frame, this decides how long the
//! frame is — the first thing a receiver must know when bytes arrive without delimiters.
//!
//! These functions lived in `wmbus::radio::rfm69_packet` and are re-exported from there
//! for compatibility. They were never radio-specific: `packet_size` is a pure function of
//! two header bytes, and filing it under a driver kept the frame layout tangled with the
//! chip that happened to deliver it. `mbus-rs` still owns the RFM69 driver; the framing
//! rule belongs to the protocol.

/// Type A sync byte, as received (bit-reversed from `0xB3` on the wire).
pub const SYNC_A: u8 = 0xCD;
/// Type B sync byte, as received (bit-reversed from `0xBC` on the wire).
pub const SYNC_B: u8 = 0x3D;

/// Normalise a sync byte, accepting either bit order.
///
/// Radios differ in whether they hand up the sync in transmission order or reversed, so
/// both spellings are mapped to the canonical one rather than requiring the caller to
/// know which chip it is talking to.
pub fn sync_norm(sync: u8) -> u8 {
    match sync {
        0xB3 => SYNC_A, // Bit-reversed A sync
        0xBC => SYNC_B, // Bit-reversed B sync
        _ => sync,
    }
}

/// Total on-air length implied by an L field.
///
/// Type B carries a single trailing CRC (`L + 2`). Type A interleaves a CRC after the
/// 10-byte first block and after each subsequent 16-byte block, so the count grows with
/// the payload — which is why a Type A frame is longer on air than its L field suggests.
///
/// The Type A total is `2 + L + num_crcs*2`, matching the deployed epulse gateway's
/// `PacketSize`. **Not** the naive `L + 3`, which undercounts multi-block CRCs and
/// truncates every frame past block 0.
fn wmbus_packet_len(l: u8, type_b: bool) -> i32 {
    let l = l as i32;
    if type_b {
        l + 2
    } else {
        // BLOCK0_LEN=10, BLOCKA_LEN=16, CRC_LEN=2
        let num_crcs = 1 + ((l - 10).max(0) + 15) / 16;
        2 + l + num_crcs * 2
    }
}

/// Determine total packet size from the first two bytes.
///
/// Handles both header arrangements, since radios differ in whether they strip the sync
/// byte before delivering: `[SYNC][LEN]` and `[LEN][SYNC]`.
///
/// Returns the total byte count, `-1` when more data is needed, or `-2` when the bytes
/// are not a wM-Bus header at all.
pub fn packet_size(data: &[u8]) -> i32 {
    if data.len() < 2 {
        return -1; // Need more data
    }

    let b0 = data[0];
    let b1 = data[1];

    // Case A/B: [SYNC][LEN]
    if sync_norm(b0) == SYNC_A || sync_norm(b0) == SYNC_B {
        let type_b = sync_norm(b0) == SYNC_B;
        return wmbus_packet_len(b1, type_b);
    }

    // Case C/D: [LEN][SYNC]
    if sync_norm(b1) == SYNC_A || sync_norm(b1) == SYNC_B {
        let type_b = sync_norm(b1) == SYNC_B;
        return wmbus_packet_len(b0, type_b);
    }

    // Not a wM-Bus header → drop
    -2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_header_arrangements_are_recognised() {
        // [SYNC][LEN] and [LEN][SYNC] must give the same answer for the same frame.
        assert_eq!(packet_size(&[SYNC_B, 10]), packet_size(&[10, SYNC_B]));
        assert_eq!(packet_size(&[SYNC_A, 10]), packet_size(&[10, SYNC_A]));
    }

    #[test]
    fn bit_reversed_sync_is_accepted_either_way() {
        assert_eq!(sync_norm(0xB3), SYNC_A);
        assert_eq!(sync_norm(0xBC), SYNC_B);
        assert_eq!(packet_size(&[0xB3, 10]), packet_size(&[SYNC_A, 10]));
    }

    #[test]
    fn type_b_is_a_single_trailing_crc() {
        assert_eq!(packet_size(&[SYNC_B, 10]), 12); // L + 2
    }

    #[test]
    fn type_a_grows_with_interleaved_block_crcs() {
        // L=10 is exactly block 0: one CRC. Longer payloads add one per 16 bytes.
        assert_eq!(packet_size(&[SYNC_A, 10]), 2 + 10 + 2);
        assert!(packet_size(&[SYNC_A, 60]) > packet_size(&[SYNC_B, 60]));
    }

    #[test]
    fn incomplete_and_foreign_headers_are_distinguished() {
        assert_eq!(packet_size(&[]), -1, "no data yet");
        assert_eq!(packet_size(&[SYNC_A]), -1, "one byte is not enough");
        assert_eq!(packet_size(&[0x00, 0x01]), -2, "not a wM-Bus header");
    }
}
