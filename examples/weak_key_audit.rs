//! Run the passive weak-key / weak-crypto audit over a capture file.
//!
//! Input: one hex-encoded wM-Bus frame per line (the `metermon-rs capture` format —
//! CRC-stripped, L-prefixed bytes as handed to the decoder). Lines that are blank or
//! start with `#` are ignored.
//!
//! Defensive fleet-hygiene only: it tests *published* default keys and key-free CTR
//! hygiene. It never brute-forces and cannot recover a full-entropy key.
//!
//! Usage: cargo run --features crypto --example weak_key_audit -- <capture.hex>

use std::io::BufRead;

use mbus_rs::wmbus::weak_key_audit::{audit_capture, Profile, Verdict};

fn parse_hex_line(line: &str) -> Option<Vec<u8>> {
    let s: String = line.split('#').next().unwrap_or("").split_whitespace().collect();
    if s.is_empty() || s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn profile_str(p: &Profile) -> String {
    match p {
        Profile::Mode5Cbc => "mode-5 CBC".into(),
        Profile::EllCtr => "ELL AES-CTR".into(),
        Profile::EllPlain => "ELL cleartext".into(),
        Profile::Unclassified(ci) => format!("CI 0x{ci:02X}"),
    }
}

fn verdict_str(v: &Verdict) -> String {
    match v {
        Verdict::DefaultKey(name) => format!("!! DEFAULT KEY ({name}) — readable by anyone"),
        Verdict::Plaintext => "!! PLAINTEXT — unencrypted on air".into(),
        Verdict::SessionReuse { sn } => format!("!! SESSION REUSE (SN {sn:#010x}) — CTR keystream reuse"),
        Verdict::NoDefaultKeyMatch => "ok  no default-key match".into(),
        Verdict::EncryptedNoWeakness => "ok  encrypted, no key-free weakness".into(),
        Verdict::Unaudited(why) => format!("--  unaudited ({why})"),
    }
}

fn main() -> std::io::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: weak_key_audit <capture.hex>");
        std::process::exit(2);
    });
    let file = std::fs::File::open(&path)?;
    let frames: Vec<Vec<u8>> =
        std::io::BufReader::new(file).lines().map_while(Result::ok).filter_map(|l| parse_hex_line(&l)).collect();

    println!("== wM-Bus passive weak-key audit ==");
    println!("capture: {path}   frames: {frames}\n", frames = frames.len());

    let results = audit_capture(&frames);
    let exposed = results.iter().filter(|m| m.verdict.is_exposure()).count();

    println!("{:>10}  {:<4}  {:<14}  {:>6}  verdict", "serial", "mfr", "profile", "frames");
    println!("{}", "-".repeat(78));
    for m in &results {
        println!(
            "{:>10}  {:<4}  {:<14}  {:>6}  {}",
            m.serial, m.mfr, profile_str(&m.profile), m.frames_seen, verdict_str(&m.verdict)
        );
    }

    println!("\n{} meter(s), {} exposure(s).", results.len(), exposed);
    if exposed == 0 {
        println!("No default-key / plaintext / SN-reuse exposure found.");
        println!("NOTE: a null result means no *tested* weakness — not a proof of key strength.");
        println!("      ELL meters are audited key-free; their per-device key strength is not assessed here.");
    }
    Ok(())
}
