# LoRa Support

mbuscrate now includes native support for LoRa modulation on the SX126x transceiver, enabling dual-mode operation alongside wM-Bus (GFSK). This allows gateways to receive from both wM-Bus and LoRa-enabled meters on a single radio, transforming payloads to JSON/BSON for MQTT forwarding. The implementation covers ~70-80% of gateway functionality: radio config/reception, OTAA/ABP frame parsing, Class A window handling, and serialization. Full LoRaWAN stack is out-of-scope (use crates like `lorawan-device` for that); this focuses on non-WAN, point-to-multipoint metering.

## Design Goals
- **Coexistence**: Prioritize wM-Bus (high density, frequent bursts) with continuous GFSK RX. Switch to LoRa only for predicted Class A windows (brief interruptions, <5% time).
- **NTP Sync**: Use absolute UTC timestamps for predictions (e.g., "TX at 14:15:00Z"), enabling precise switches without drift.
- **Mains-Powered**: No strict duty cycle on gateway; continuous polling OK. Devices report schedules; misses tolerated via cumulative diffs.
- **Custom Protocol**: Non-LoRaWAN—custom downlinks for steering/triggers (CBOR payloads). OTAA/ABP for join, but simplified (no ADR, no server handshake).
- **Extensibility**: Modular (radio core + gateway layer); optional features (NTP, MQTT, CBOR).
- **No Breakage**: Existing wM-Bus API unchanged; LoRa opt-in via new enums/variants.

## Key Features
- **LoRa PHY**: Full SX126x LoRa support (SF7-12, BW 7.8-500kHz, CR 4/5-4/8, explicit/implicit headers, CRC).
- **OTAA/ABP Parsing**: Extract DevEUI/AppEUI (OTAA), DevAddr/NwkSKey (ABP) from frames; custom schedule reporting (interval, class).
- **Class A Handling**: Predict RX windows (1s after TX + 2s delay); switch radio for downlinks/triggers.
- **Dual-Mode Gateway**: Weighted polling (90% GFSK), NTP-synced scheduling (BinaryHeap for LoRa windows), cumulative delta calcs.
- **Payload Transformation**: Unified `MeterData` struct with `to_json()`/`to_bson()`; diff for missed packets.
- **Steering/Triggering**: Custom downlinks (e.g., "TX now at freq X") in Class A windows; MQTT-triggered.
- **NTP Integration**: Async client for UTC sync; timestamps in all data.
- **MQTT Sink**: Optional publishing (JSON/BSON) to topics like "meters/{device_id}"; subscribe for steering.

## Architecture Overview
```
Gateway App (e.g., examples/dual_mode_gateway.rs)
├── GatewayRadio (src/wmbus/gateway/gateway_radio.rs)
│   ├── Sx126xDriver<H> (src/wmbus/radio/driver.rs)  # Dual-mode radio
│   ├── NtpClient (src/wmbus/gateway/ntp_client.rs)  # UTC sync
│   ├── DeviceSchedule (HashMap + BinaryHeap)        # Predictions
│   └── GatewaySink (trait + MQTT impl)              # Output (JSON/BSON + MQTT)
│
├── LoRa Submodule (src/wmbus/radio/lora/)
│   ├── params.rs: LoRaModParams, LoRaPacketParams, enums (SF, BW, CR)
│   └── packet.rs: decode_lora_packet(), parse_otaa_join(), parse_abp_data(),
│                build_trigger_frame(), class_a_windows()
│
└── MeterData (src/wmbus/gateway/meter_data.rs): Unified wM-Bus/LoRa data,
    cumulative diffs, serialization
```

### Radio Layer (SX126x Dual-Mode)
- **Switching**: Explicit via `set_packet_type(PacketType::LoRa/Gfsk)` (opcode 0x8A, ~20ms). Use `switch_to_lora_mode()` / `switch_to_gfsk_mode()` for convenience.
- **Configuration**:
  - `configure_for_lora(freq_hz, sf, bw, cr, power_dbm)`: Sets LoRa params (opcode 0x8B/0x8C), sync word (0x0741 for private).
  - `configure_for_wmbus(freq_hz, bitrate)`: Existing GFSK (unchanged).
- **Reception (`process_irqs()`)**: Returns `UnifiedPacket { mode: PacketType, payload: Vec<u8>, status: PacketStatus }` (GFSK: RSSI; LoRa: RSSI/SNR).
- **Transmission**:
  - `lbt_transmit(data, config)`: For uplinks (wM-Bus/LoRa).
  - `send_trigger_downlink(device_addr, payload, confirm)`: For downlinks (Class A windows), builds LoRa frame (MHDR=0x40, FPort=0xFF, CBOR trigger).
- **State**: Tracks `current_packet_type` for mode detection.

### LoRa Submodule
- **Params**: Enums/structs for SF/BW/CR; helpers for bitrate/ToA calcs (datasheet formulas).
- **Packet Parsing**:
  - `decode_lora_packet(payload, status) -> Result<LoRaPayload, LoRaError>`: MHDR validation (0x00 JoinReq, 0x20/0x80 DataUp).
  - `parse_otaa_join(payload) -> Result<JoinRequest, LoRaError>`: Extracts DevEUI (8B), AppEUI (8B), DevNonce (2B), schedule (CBOR: { "tx_interval_min": 15, "class": "A" }).
  - `parse_abp_data(payload) -> Result<DataPayload, LoRaError>`: Extracts DevAddr (4B), FCtrl, FPort, FRMPayload (meter data + schedule).
  - `build_trigger_frame(device_addr, payload) -> Vec<u8>`: MHDR=0x40 downlink with CBOR (e.g., { "cmd": "tx_now", "freq_hz": 868500000 }).
  - Cumulative: `calc_delta(new: f64, last: Option<f64>) -> f64` for missed packets.
- **Class A**: `class_a_windows(tx_end: Instant, sf: SpreadingFactor) -> (Instant, Instant)` (1s/2s delays per Table 13-79).

### Gateway Layer
- **NTP Client**: Async sync every 5min (`ntp` crate); UTC for predictions/timestamps.
- **MeterData**: Unified struct with `cumulative_delta`, `next_tx_utc`, `class`. `to_json()` / `to_bson()` include UTC, mode, delta.
- **GatewayRadio**:
  - Fields: `driver`, `ntp_client`, `schedule: HashMap<String, DeviceSchedule>`, `sink: Box<dyn GatewaySink>`.
  - `DeviceSchedule { device_id, next_tx_utc: OffsetDateTime, interval_min: u32, class: Option<LoRaClass>, mode: PacketType }`.
  - `receive_loop(config: &GatewayConfig) -> impl Stream<Item = Result<MeterData, GatewayError>>`: Async (tokio).
    - Sync NTP.
    - Weighted poll: Continuous GFSK (poll every 20ms); switch to LoRa for windows (if <2s away).
    - On packet: Parse mode, update schedule (next_tx = now + interval), calc delta.
    - Miss handling: Set delta=0, flag "possible_miss: true".
  - `steer_device(device_id, freq_hz, duty_pct, sf) -> Result<(), GatewayError>`: TX downlink in predicted window, update schedule.
  - `trigger_tx(device_id, cmd: &str) -> Result<(), GatewayError>`: Send trigger (CBOR), RX for ACK in Window 2.
- **GatewaySink Trait**: `send(data: &MeterData, topic: &str) -> Result<(), SinkError>`.
  - `MqttGatewaySink`: Publishes JSON/BSON; subscribes "steer/#" for commands.

### Usage Example
```rust
use mbuscrate::wmbus::gateway::{GatewayRadio, GatewayConfig, MqttGatewaySink};
use mbuscrate::wmbus::radio::hal::raspberry_pi::RaspberryPiHal;
use time::OffsetDateTime;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hal = RaspberryPiHal::new(0, &GpioPins::default())?;
    let mut driver = Sx126xDriver::new(hal, 32_000_000);
    let config = GatewayConfig {
        wmbus_freq_hz: 868_950_000,
        lora_sf: Some(SpreadingFactor::SF10),
        lora_bw: LoRaBandwidth::BW125,
        lora_cr: CodingRate::CR4_5,
        ntp_server: "pool.ntp.org".to_string(),
        mqtt_broker: Some("mqtt://localhost:1883".to_string()),
        gfsk_ratio: 0.9,  // 90% GFSK
        switch_buffer_sec: 2,
    };

    let mut gateway = GatewayRadio::new(driver, config)?;
    let mut sink = MqttGatewaySink::new("mqtt://localhost:1883", None, None)?;

    // Subscribe for steering commands
    sink.subscribe_steering("steer/#", |topic, payload| {
        // Parse payload (CBOR/JSON), call gateway.steer_device
        println!("Steer command: {} -> {}", topic, payload);
    });

    // Main loop
    while let Some(data) = gateway.receive_loop().await? {
        sink.send(&data, &format!("meters/{}", data.device_id)).await?;
        if data.cumulative_delta == 0.0 {
            log::warn!("Possible missed packet for {}", data.device_id);
        }
    }
    Ok(())
}
```

### Dependencies
- `time = "0.3"` (UTC/OffsetDateTime).
- `ntp = "0.3"` (NTP queries, features=["ntp"]).
- `tokio = { version = "1.38", features = ["full"] }` (async loop).
- `rumqttc = { version = "0.28", optional = true, features = ["tls"] }` (MQTT, features=["mqtt"]).
- `serde_cbor = "0.11"` (CBOR for triggers, features=["cbor"]).
- `ciborium = "0.2"` (CBOR ser/de).

### Testing and Validation
- **Unit Tests**: Param calcs, parsing (golden LoRa frames), window timing.
- **Integration**: Mock driver for switches (e.g., simulate Class A TX in window).
- **Real-World**: Use signal generator for wM-Bus/LoRa; measure loss with 100 devices.
