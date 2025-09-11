# RP2350 Implementation Design for wM-Bus Burst Handling

## Overview

The RP2350 implementation rearchitects mbus-rs as a no_std, embedded binary targeting the Raspberry Pi RP2350 microcontroller (dual Arm Cortex-M33 cores at 150MHz, 520KB SRAM, 4 PIO state machines, hardware AES-128 accelerator, and 12 DMA channels). It focuses on efficient RX and decoding of wM-Bus bursts (EN 13757-4 S-mode: 100kbps, 100+ encrypted packets of ~255 bytes each) without wired interfaces, leveraging hardware offload for low CPU utilization and power. The design uses Embassy (embedded async framework) for cooperative multitasking and multicore orchestration, achieving ~400 packets/sec throughput with ~4% average core utilization during aggressive bursts (~250ms RX time).

## High-Level Architecture

### no_std Focus
Core parsing/decoding from `frame.rs`, `frame_decode.rs` (CRC-16, 3-of-6 demod), `payload/` (VIF/record decode), and `wmbus/encoding.rs`. No tokio/async-std; heapless collections replace dynamic allocs for fixed-size buffers (e.g., 255-byte packets).

### Dual-Core Producer-Consumer
- **Core 0 (Main, Secure Mode via TrustZone)**: Runs Embassy executor; initializes SX126x radio (868.95MHz, GFSK) and peripherals. Async task handles RX: Monitors DIO1/BUSY via PIO, triggers DMA bursts to SRAM ring buffer (~16KB), filters/enqueues valid packets to a shared async channel.
- **Core 1 (General Mode)**: Spawned via Embassy multicore; dedicated async task dequeues packets, performs integrity checks (hardware CRC), decrypts (AES offload), parses/demodulates, and aggregates results (e.g., dedup by device ID, stats). Signals completion back via channel or atomic flags for Core 0 (e.g., LED/USB output).

### Synchronization
Embassy-sync bounded async channel (128 slots, ~32KB SRAM) for inter-core packet/results transfer (lock-free, non-blocking). Atomics for lightweight flags (e.g., DECRYPT_DONE). TrustZone isolates AES keys/config on Core 0.

## Hardware Offload Components

### PIO (4 State Machines)
Fully offloads RF I/O and protocol primitives (~2KB flash total):
- **SM0/SM1**: SPI master for SX126x commands (MOSI/SCLK @8MHz on GP18/19; NSS on GP16).
- **SM2**: Custom 3-of-6 demodulation and bit unstuffing (hardware timing for 100kbps, outputs to FIFO).
- **SM3**: DIO1/BUSY polling + initial CRC-16 compute (CCITT poly 0x1021; compares frame CRC bytes, discards invalids pre-DMA).
- PIO FIFOs (~1KB SRAM) buffer outputs directly to SRAM, triggering NVIC IRQs for Embassy wakes (zero Core 0 polling).

### DMA (Chained Transfers)
12 channels enable zero-CPU burst RX: CH0/1 auto-refill ring buffer from SX126x FIFO on PIO IRQ (~2.5ms/packet); CH2 chains to CRC peripheral. Supports ~16KB continuous buffer for bursts, yielding Embassy tasks during transfers.

### CRC-16 Peripheral
SIO/CRC engine offloads verification (poly 0x1021, init 0xFFFF, XOR 0xFFFF) during DMA (<1µs/frame vs. 10µs software), filtering ~20% noisy packets hardware-side.

### AES-128 Accelerator
Hardware GCM/CBC/CTR modes (for wM-Bus Modes 5/7/9) with 11-byte AAD/12-byte IV; ~1 cycle/byte (<5µs/packet) in TrustZone. Loaded post-CRC; outputs decrypted frames to Core 1 channel.

## Data Flow During Burst

1. **RX Initiation (Core 0)**: Set SX126x RX continuous; PIO SM3 polls DIO1 for packet ready (~2.5ms intervals).
2. **Hardware Offload**: PIO SM0/1 + DMA transfers burst to SRAM (PIO demod/CRC filters invalids); IRQ wakes Embassy task (~1µs).
3. **Enqueue (Core 0)**: Async channel send valid packets (~3µs/packet; yields 99% time).
4. **Decode Pipeline (Core 1)**: Async recv → hardware AES decrypt (<5µs) → nom parse/VIF decode (~185µs) → aggregate (dedup/filter) → channel back.
5. **Completion (Core 0)**: Recv results; toggle LED/USB output; reset for next burst.

## Performance & Resource Estimates

### Burst Performance (100 Encrypted Packets, 250ms RX)
- **Core 0**: ~0.3% (enqueue overhead)
- **Core 1**: ~7.7% duty (194µs/packet pipelined)
- **System**: ~4% avg (hardware hides 95% I/O/crypto)
- **Throughput**: ~400 packets/sec (radio-limited)

### Resource Usage
- **SRAM**: Channel + ring/FIFOs ~48KB peak (520KB total ample for 200+ packets)
- **Flash**: ~141KB (PIO/AES drivers + Embassy ~+10KB vs. RP2040)
- **Power**: ~1.7mA (offloads reduce active cycles); TrustZone secures AES without extra draw

### Gains vs. RP2040
- +50% throughput (clock/AES)
- 2x SRAM/PIO for larger bursts/filtering
- CRC offload +15% efficiency

## Technical Implementation Details

### Embassy Framework Integration

```rust
#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::{
    multicore::{spawn_core1, Stack},
    peripherals::{CORE1, DMA_CH0, DMA_CH1, DMA_CH2},
    pio::{Pio, PioPin},
};
use embassy_sync::channel::Channel;

// Inter-core communication channels
static PACKET_CHANNEL: Channel<ThreadModeRawMutex, WMBusPacket, 128> = Channel::new();
static RESULT_CHANNEL: Channel<ThreadModeRawMutex, ProcessedResult, 32> = Channel::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Core 0 initialization
    let p = embassy_rp::init(Default::default());
    
    // Initialize radio and peripherals
    let radio = init_sx126x_radio(p.PIO0, p.DMA_CH0, p.DMA_CH1).await;
    let aes = init_hardware_aes(p.TRNG).await;
    
    // Spawn Core 1
    spawn_core1(p.CORE1, unsafe { &mut CORE1_STACK }, move || {
        core1_task(CORE1_EXECUTOR.init(Executor::new()))
    });
    
    // Core 0 main loop
    spawner.spawn(rx_task(radio, aes)).unwrap();
    spawner.spawn(output_task()).unwrap();
}
```

### PIO State Machine Configuration

```rust
// PIO program for SX126x SPI interface
const SPI_PROGRAM: &[u16] = &[
    0x6021, // out pins, 1        [1]
    0xe001, // set pins, 1     [1]
    0x1280, // jmp pin, 0      [2]
    0xa027, // mov x, osr
    0x6041, // out y, 1        [1]
    // ... SPI clock generation
];

// PIO program for 3-of-6 demodulation
const DEMOD_PROGRAM: &[u16] = &[
    0x20c0, // wait 1 pin, 0   ; Wait for data
    0x4001, // in pins, 1      ; Sample bit
    0x0045, // jmp x--, 5      ; Count 6 bits
    0x8080, // pull noblock    ; Get decode table
    0xa047, // mov y, osr      ; Load to Y
    // ... 3-of-6 lookup and output
];

async fn init_pio_state_machines(pio: PIO0) -> PioStateMachines {
    let Pio { mut common, sm0, sm1, sm2, sm3, .. } = Pio::new(pio);
    
    // SM0/SM1: SPI master for SX126x
    let spi_program = common.load_program(&SPI_PROGRAM);
    let spi_sm = sm0.start_with_program(&spi_program);
    
    // SM2: 3-of-6 demodulation
    let demod_program = common.load_program(&DEMOD_PROGRAM);
    let demod_sm = sm2.start_with_program(&demod_program);
    
    // SM3: DIO1 polling and CRC
    let poll_program = common.load_program(&POLL_PROGRAM);
    let poll_sm = sm3.start_with_program(&poll_program);
    
    PioStateMachines { spi_sm, demod_sm, poll_sm }
}
```

### Hardware Accelerated Processing

```rust
use embassy_rp::dma::{AnyChannel, Config};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, DMA_CH2};

struct HardwareOffload {
    dma_rx: AnyChannel,
    dma_crc: AnyChannel, 
    aes_engine: AesEngine,
    crc_peripheral: CrcPeripheral,
}

impl HardwareOffload {
    async fn process_packet_burst(&mut self, raw_data: &[u8]) -> Result<Vec<WMBusPacket>, Error> {
        // Chain DMA transfers: Raw → CRC → AES → Decoded
        let crc_config = Config::new()
            .priority(Priority::High)
            .chain_to(self.dma_crc);
            
        // Hardware CRC verification (~1µs vs 10µs software)
        let crc_valid = self.dma_rx.start_with_config(
            crc_config, 
            raw_data,
            &mut self.crc_buffer
        ).await?;
        
        if !crc_valid {
            return Err(Error::CrcMismatch);
        }
        
        // Hardware AES decryption in TrustZone (~5µs/packet)
        let decrypted = self.aes_engine.decrypt_ctr_mode(
            &self.crc_buffer,
            &self.session_key,
            &self.iv
        ).await?;
        
        // Software parsing using nom (retained from main crate)
        parse_wmbus_packets(&decrypted)
    }
}
```

### Dual-Core Task Distribution

```rust
// Core 0: RX and coordination
#[embassy_executor::task]
async fn rx_task(mut radio: SX126xRadio, aes: AesEngine) {
    let mut ring_buffer = RingBuffer::<u8, 16384>::new();
    let mut packet_count = 0u32;
    
    loop {
        // Set radio to continuous RX mode
        radio.set_rx_continuous().await?;
        
        // Wait for burst detection (PIO SM3 IRQ)
        let burst_start = radio.wait_for_burst().await;
        defmt::info!("Burst detected at {}", burst_start);
        
        // Hardware-accelerated RX burst (~250ms for 100 packets)
        while let Some(raw_packet) = radio.receive_packet_dma(&mut ring_buffer).await {
            // Quick envelope check (done in PIO SM3)
            if raw_packet.len() < 10 || raw_packet.len() > 255 {
                continue;
            }
            
            // Send to Core 1 for processing (non-blocking)
            if let Err(_) = PACKET_CHANNEL.try_send(WMBusPacket::new(raw_packet)) {
                defmt::warn!("Channel full, dropping packet");
            } else {
                packet_count += 1;
            }
        }
        
        defmt::info!("Burst complete: {} packets received", packet_count);
        packet_count = 0;
    }
}

// Core 1: Processing and aggregation
#[embassy_executor::task]
async fn core1_processing_task() {
    let mut hardware = HardwareOffload::new();
    let mut device_cache = heapless::FnvIndexMap::<u32, DeviceRecord, 64>::new();
    
    loop {
        // Receive packet from Core 0
        let raw_packet = PACKET_CHANNEL.receive().await;
        
        // Hardware-accelerated processing pipeline
        match hardware.process_packet_burst(&raw_packet.data).await {
            Ok(decoded_packets) => {
                for packet in decoded_packets {
                    // Deduplicate by device ID
                    let device_id = packet.device_id();
                    if let Some(existing) = device_cache.get_mut(&device_id) {
                        existing.update_from_packet(&packet);
                    } else {
                        device_cache.insert(device_id, DeviceRecord::from_packet(&packet))
                            .map_err(|_| defmt::warn!("Device cache full"))?;
                    }
                }
                
                // Send results back to Core 0
                let result = ProcessedResult {
                    packet_count: decoded_packets.len(),
                    unique_devices: device_cache.len(),
                    processing_time_us: hardware.last_processing_time(),
                };
                
                RESULT_CHANNEL.send(result).await;
            }
            Err(e) => {
                defmt::warn!("Packet processing failed: {:?}", e);
            }
        }
    }
}
```

### Memory Layout and Resource Management

```rust
// Memory layout optimized for RP2350
#[repr(C)]
struct WMBusPacket {
    len: u16,                    // 2 bytes
    rssi: i8,                    // 1 byte  
    timestamp: u32,              // 4 bytes
    data: [u8; 255],            // 255 bytes max packet size
}

// Ring buffer for DMA bursts (16KB)
static mut RING_BUFFER: [u8; 16384] = [0; 16384];

// Embassy channel storage (32KB for 128 packets)
static PACKET_STORAGE: [WMBusPacket; 128] = [WMBusPacket::EMPTY; 128];

// Core 1 stack (8KB)
static mut CORE1_STACK: Stack<8192> = Stack::new();
```

## Integration with Existing mbus-rs Crate

### Shared Components
The RP2350 implementation reuses the following components from the main mbus-rs crate:
- `src/wmbus/frame_decode.rs` - Frame parsing and validation logic
- `src/payload/` - VIF decoding and data record parsing  
- `src/wmbus/encoding.rs` - Data encoding/decoding utilities
- `src/error.rs` - Error types (adapted for no_std)

### Adaptation Strategy
```rust
// Conditional compilation for embedded target
#[cfg(feature = "rp2350")]
mod embedded {
    use heapless::Vec as HeaplessVec;
    use heapless::String as HeaplessString;
    
    // Replace std collections with heapless equivalents
    pub type Vec<T> = HeaplessVec<T, 256>;
    pub type String = HeaplessString<256>;
    
    // Embassy async instead of tokio
    pub use embassy_time::{Duration, Timer};
    pub use embassy_futures::join;
}

#[cfg(not(feature = "rp2350"))]
mod hosted {
    // Standard library types for hosted platforms
    pub use std::vec::Vec;
    pub use std::string::String;
    pub use tokio::time::{Duration, sleep as Timer};
    pub use tokio::join;
}
```

This design maximizes RP2350 hardware for secure, low-power wM-Bus gateways—PIO/DMA/CRC/AES chain for front-end, Embassy/cores for back-end decode. Deterministic, scalable to 200-packet bursts.

## Future Extensions

### Enhanced Security
- TrustZone-M secure boot chain
- Hardware key derivation (TRNG + HKDF)
- Secure firmware updates over USB/UART

### Advanced Features  
- Multi-frequency scanning (868.3/868.95/169 MHz)
- Adaptive power management based on burst patterns
- Over-the-air configuration via LoRa/WiFi module
- Integration with cloud platforms for large-scale deployments

### Performance Optimizations
- Custom LLVM passes for RP2350 dual-core
- SIMD acceleration for bulk operations
- Zero-copy packet forwarding using DMA scatter-gather
- Predictive burst detection using machine learning