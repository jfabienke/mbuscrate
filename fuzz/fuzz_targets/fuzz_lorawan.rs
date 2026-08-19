#![no_main]

//! Fuzz the LoRaWAN join/data-frame layer: the parsers eat untrusted radio bytes
//! and must never panic, and everything `build_data_down` produces must parse back
//! and authenticate (the round-trip invariant that ties the builder to the parser).

use libfuzzer_sys::fuzz_target;
use mbus_rs::lorawan::{
    build_data_down, build_join_accept, derive_session_keys, link_adr_req, parse_link_adr_ans,
    DataFrame, DownlinkParams, JoinAcceptParams, JoinRequest,
};

fn u32_at(data: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([
        data.get(i).copied().unwrap_or(0),
        data.get(i + 1).copied().unwrap_or(0),
        data.get(i + 2).copied().unwrap_or(0),
        data.get(i + 3).copied().unwrap_or(0),
    ])
}

fuzz_target!(|data: &[u8]| {
    // 1. Parsers must handle any input gracefully — no panic, no out-of-bounds.
    let _ = JoinRequest::parse(data);
    let _ = parse_link_adr_ans(data);
    let parsed = DataFrame::parse(data);

    // A frame that parsed must survive MIC-check and decrypt with arbitrary keys.
    if let Ok(df) = &parsed {
        let key = [0u8; 16];
        let _ = df.verify_mic(&key, u32_at(data, 0));
        let _ = df.decrypt_payload(&key, &key, df.fcnt as u32);
    }

    // 2. Round-trip invariant: whatever build_data_down emits must parse back as a
    //    downlink, carry the exact FOpts, and verify under the network key. A break
    //    here is a builder/parser or MIC mismatch — exactly what fuzzing should catch.
    if data.len() >= 8 {
        let dev_addr = u32_at(data, 0);
        let fcnt = u32_at(data, 4);
        let fopts_len = (data[6] as usize) % 16; // build asserts <= 15
        let fopts: Vec<u8> = data.iter().copied().take(fopts_len).collect();
        let nwk = [0x11u8; 16];
        let app = [0x22u8; 16];
        let p = DownlinkParams {
            dev_addr,
            fcnt,
            adr: data[7] & 1 == 1,
            ack: data[7] & 2 == 2,
            fpending: data[7] & 4 == 4,
            fopts: fopts.clone(),
            fport: None,
            frm_payload: Vec::new(),
        };
        let frame = build_data_down(&nwk, &app, &p);
        let df = DataFrame::parse(&frame).expect("a built downlink must always parse");
        assert!(!df.is_uplink(), "downlink must not read as uplink");
        assert_eq!(df.dev_addr, dev_addr, "DevAddr must round-trip");
        assert_eq!(df.fcnt, fcnt as u16, "FCnt low-16 must round-trip");
        assert_eq!(df.fopts, fopts, "FOpts must round-trip (cleartext in 1.0.x)");
        assert!(df.verify_mic(&nwk, fcnt), "built downlink MIC must verify");
    }

    // 3. LinkADRReq encoding is always well-formed: CID 0x03, mask round-trips.
    if data.len() >= 5 {
        let mask = u16::from_le_bytes([data[0], data[1]]);
        let cmd = link_adr_req(mask, data[2] & 0x0F, data[3] & 0x0F, data[4] & 0x0F);
        assert_eq!(cmd[0], 0x03, "LinkADRReq CID");
        assert_eq!(u16::from_le_bytes([cmd[2], cmd[3]]), mask, "ChMask round-trip");
    }

    // 4. JoinAccept build + key derivation must not panic on any params.
    if data.len() >= 8 {
        let params = JoinAcceptParams {
            app_nonce: u32_at(data, 0) & 0x00FF_FFFF,
            net_id: u32_at(data, 3) & 0x00FF_FFFF,
            dev_addr: u32_at(data, 0),
            dl_settings: data[6],
            rx_delay: data[7],
        };
        let app_key = [0x33u8; 16];
        let _ = build_join_accept(&app_key, &params);
        let _ = derive_session_keys(&app_key, &params, u16::from_le_bytes([data[6], data[7]]));
    }
});
