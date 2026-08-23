//! Links `mbus-core` for a bare-metal target so panic-freedom can be *measured*.
//!
//! The check is the linker map, not the source: if any reachable path can panic, the
//! core formatting machinery (`core::fmt::write`, `panic_fmt`, `slice_index_fail`) is
//! pulled in and shows up in the symbols. Grepping an `.rlib` proves nothing, because an
//! unlinked library leaves those references unresolved.
#![no_std]
#![no_main]

use core::panic::PanicInfo;

/// Deliberately minimal: no formatting, no unwinding. If the build succeeds and the map
/// is clean, nothing on the exercised paths reached for a panic.
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // The detector. If ANY code in mbus-core can reach a panic, this handler is live and
    // the link fails on an undefined symbol. If every panic path was eliminated, the
    // handler is dead code, --gc-sections drops it, and the link succeeds.
    //
    // Do not "fix" a failing link by defining this symbol -- the failure IS the result.
    //
    // Why not scan the binary for panic strings: a handler that ignores its PanicInfo lets
    // LLVM drop the message formatting, so a REAL out-of-bounds index leaves no string
    // behind. That check passed a deliberately-panicking negative control. This one does not.
    extern "C" {
        fn MBUS_CORE_CAN_PANIC_SEE_PANIC_PROBE_README() -> !;
    }
    unsafe { MBUS_CORE_CAN_PANIC_SEE_PANIC_PROBE_README() }
}

/// Touch the parsing and crypto entry points so they cannot be dead-code-eliminated.
/// `black_box` would be better but is unstable here; `read_volatile` on the result is
/// enough to keep the calls.
#[no_mangle]
pub extern "C" fn exercise(raw: *const u8, len: usize) -> u32 {
    let bytes = unsafe { core::slice::from_raw_parts(raw, len) };
    let mut acc = 0u32;

    if let Ok(jr) = mbus_core::lorawan::JoinRequest::parse(bytes) {
        acc = acc.wrapping_add(jr.dev_nonce as u32);
        if jr.verify_mic(&[0u8; 16]) {
            acc = acc.wrapping_add(1);
        }
    }
    if let Ok(df) = mbus_core::lorawan::DataFrame::parse(bytes) {
        acc = acc.wrapping_add(df.fcnt as u32);
        if df.verify_mic(&[0u8; 16], 0) {
            acc = acc.wrapping_add(1);
        }
    }
    if let Some(v) = mbus_core::lorawan::parse_link_adr_ans(bytes) {
        acc = acc.wrapping_add(v as u32);
    }

    // The full wM-Bus + M-Bus decode pipeline: radio/serial bytes to a parsed reading.
    // This is what the whole port exists for, and these parsers handle the most complex
    // untrusted input in the crate — the record parser is where this session found an
    // out-of-bounds write reachable from a hostile frame. Exercising them here makes
    // "no meter frame can panic the decoder" a LINKED property: if any reachable path in
    // any stage can panic, this binary fails to link and the ratchet goes red.
    acc = acc.wrapping_add(mbus_core::wmbus::framing::packet_size(bytes) as u32);
    if let Ok(frame) = mbus_core::wmbus::frame::parse_wmbus_frame(bytes) {
        acc = acc.wrapping_add(frame.device_address);
    }
    if let Ok(link) = mbus_core::wmbus::mode_c::decode_mode_c(bytes) {
        acc = acc.wrapping_add(link.device_address);
    }
    if let Ok(blocks) = mbus_core::wmbus::block::verify_blocks(bytes) {
        acc = acc.wrapping_add(blocks.len() as u32);
    }
    if let Ok((rec, used)) = mbus_core::payload::record::parse_variable_record_consumed(bytes) {
        acc = acc.wrapping_add(used as u32);
        acc = acc.wrapping_add(rec.value.as_f64() as u32);
    }
    if let Ok(rec) = mbus_core::payload::record::parse_fixed_record(bytes) {
        acc = acc.wrapping_add(rec.storage_number);
    }
    acc
}

/// `_start` must actually CALL `exercise`, or the linker garbage-collects it and the
/// binary ends up empty — at which point "no panic symbols" is true and meaningless.
/// The first version of this probe made exactly that mistake: 732 bytes, zero symbols,
/// a clean result that measured nothing.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // A fixed buffer in .data so the call cannot be constant-folded away.
    static INPUT: [u8; 32] = [
        0x00, 0x8b, 0xb0, 0x49, 0xa8, 0x5f, 0x36, 0xb8, 0x62, 0x75, 0x77, 0x25, 0x80, 0xfc,
        0x48, 0xb6, 0x04, 0xba, 0xf9, 0xca, 0xde, 0xa3, 0x4f, 0x11, 0x22, 0x33, 0x44, 0x55,
        0x66, 0x77, 0x88, 0x99,
    ];
    let r = exercise(INPUT.as_ptr(), INPUT.len());
    unsafe { core::ptr::write_volatile(0x2000_0000 as *mut u32, r) };
    loop {}
}
