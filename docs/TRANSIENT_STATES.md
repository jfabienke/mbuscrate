# Transient States in mbus-rs

## Overview

All states in `mbus-rs` are **transient/ephemeral by design**—they exist only in memory during runtime and are automatically dropped when their owning struct goes out of scope or the program exits. There is **no indefinite persistence** (e.g., no globals that survive restarts without user intervention). "Persistence" here refers to **runtime lifetime** (how long the data survives in memory before eviction, reset, or drop). This is controlled by Rust's ownership: states are tied to structs, and users decide when to create/drop/reset them.

### Key Clarifications

- **No Disk/File/DB**: Crate has zero I/O for saving state—users add persistence externally (e.g., serialize to JSON/DB on shutdown, reload on startup)
- **Eviction/Reset**: Many states auto-evict old data (e.g., fixed-size queues) or have explicit reset methods (e.g., `clear()`)
- **Typical Lifetimes**: Depend on usage:
  - **Short (ms-seconds)**: Local vars/buffers (per operation)
  - **Medium (seconds-minutes)**: Queues/buffers (until processed)
  - **Long (minutes-hours+)**: Histories in long-lived structs (e.g., in a daemon process until restart)
- **Measurement**: Based on code (e.g., VecDeque capacity) and typical gateway usage (e.g., 24/7 loop)

## State Categories by Lifetime

### Short-Lived States (Milliseconds to Seconds; Per-Operation)

These are local to functions/methods—exist briefly for computation, then dropped. They don't "persist" beyond the call.

| State                   | Location                | Purpose                        | Lifetime                |
|-------------------------|-------------------------|--------------------------------|-------------------------|
| `BitRev::temp_buffer`   | `util/bitrev.rs`        | Scratch space for bit reversal | ~10µs per `rev8()` call |
| `FrameDecoder::buffer`  | `wmbus/frame_decode.rs` | Temp frame reassembly buffer   | ~1-10ms per frame       |
| `IoBuffer::data`        | `util/iobuffer.rs`      | Serial data buffer             | ~ms per byte stream     |
| `SerialMock::rx_buffer` | `mbus/serial_mock.rs`   | Mock RX stream buffer          | ~seconds in tests       |
| `SerialMock::tx_queue`  | `mbus/serial_mock.rs`   | Mock TX responses              | ~seconds in tests       |
| `EventQueue::events`    | `lora/irq_queue.rs`     | Pending IRQs                   | ~ms per IRQ             |

**Management**: Automatic - dropped at method end. No user control needed.

### Medium-Lived States (Seconds to Minutes; Until Processed/Reset)

These hold data until explicitly cleared or evicted (e.g., fixed capacity). Useful for buffering but not long-term.

| State                      | Location                   | Purpose                | Lifetime                 | Eviction            |
|----------------------------|----------------------------|------------------------|--------------------------|---------------------|
| `MbusProtocol::state`      | `mbus/mbus_protocol.rs`    | State machine position | ~seconds per transaction | Reset per frame     |
| `CompactCache::cache`      | `wmbus/compact_cache.rs`   | wM-Bus frame cache     | ~minutes                 | LRU (capacity 128)  |
| `LogThrottle::counters`    | `util/logging.rs`          | Log cooldown timers    | 1s-1min                  | Timeout expiry      |
| `WindowedCounter::windows` | `instrumentation/stats.rs` | Sliding error window   | ~10 minutes              | 60s windows, max 10 |

**Management**: Auto-evicted by time/size. User resets via `clear()` for fresh starts.

### Long-Lived States (Minutes to Indefinite; Program Lifetime)

These are in top-level structs (e.g., controllers) that users keep alive in loops (e.g., gateway daemon). They can last **indefinitely** (until program restart/shutdown), but are periodically reset/evicted by design or user calls. These are the **longest transient states**.

| State                            | Location                   | Purpose                                 | Lifetime   | Eviction Policy                |
|----------------------------------|----------------------------|-----------------------------------------|------------|--------------------------------|
| `AdrController::metrics_history` | `lora/adr.rs`              | RSSI/SNR history for SF/power decisions | Indefinite | FIFO, 20 samples max           |
| `ChannelHopper::quality_history` | `lora/channel_hopping.rs`  | Per-channel quality samples             | Indefinite | FIFO, 100/channel              |
| `ChannelHopper::blacklist`       | `lora/channel_hopping.rs`  | Blacklisted channels                    | Indefinite | Manual via `clear_blacklist()` |
| `SmartDecoder::device_stats`     | `lora/smart_decoder.rs`    | Per-device format success/failure       | Indefinite | Manual via `clear_stats()`     |
| `LoRaDeviceManager::decoders`    | `lora/decoder.rs`          | Device-decoder registry                 | Indefinite | Manual clear                   |
| `DutyCycleLimiter::band_usage`   | `lora/duty_cycle.rs`       | TX time per band                        | ~1 hour    | Manual reset                   |
| `DeviceStats::error_counters`    | `instrumentation/stats.rs` | Per-device error history                | Indefinite | WindowedCounter eviction       |

**Longest-Lived States**: Histories in controllers (e.g., `ChannelHopper::quality_history`, `SmartDecoder::device_stats`)

- **Duration**: **Indefinite** (program lifetime, e.g., days/weeks in a 24/7 gateway daemon until shutdown/restart)
- **Size Limits**: Auto-eviction to prevent unbounded growth (e.g., 100 samples ~10min at high rate)
- **Reset Options**: User calls methods like `clear_stats()` or creates new instance
- **Typical Usage**: ~1-7 days before manual reset (e.g., daily cron), but can be indefinite for long-term trends

## Memory Management

### Eviction Strategies

1. **FIFO (First In, First Out)**
   - Used by: `AdrController`, `ChannelHopper`
   - Example: Keep last 20 RSSI samples, drop oldest
   ```rust
   if self.metrics_history.len() >= self.averaging_window {
       self.metrics_history.pop_front();
   }
   self.metrics_history.push_back(metrics);
   ```

2. **LRU (Least Recently Used)**
   - Used by: `CompactCache`
   - Example: Evict least-accessed frame when cache full

3. **Time-Based**
   - Used by: `WindowedCounter`, `LogThrottle`
   - Example: Drop entries older than 60 seconds

4. **Manual Reset**
   - Used by: `SmartDecoder`, `DutyCycleLimiter`
   - Example: User calls `clear_stats()` or `reset_band_usage()`

### Memory Overhead

| Component           | Per-Device Memory | Notes                         |
|---------------------|-------------------|-------------------------------|
| `DeviceStats`       | ~1KB              | Error counters, success rates |
| `LoRaDeviceManager` | ~100 bytes        | Decoder type registration     |
| `ChannelHopper`     | ~10KB total       | 100 samples × 16 channels     |
| `SmartDecoder`      | ~1KB/device       | Format detection stats        |

**Total for 100 devices**: ~100-200KB (negligible for modern systems)

## Persistence Patterns

The crate provides no built-in persistence. Users implement based on needs:

### Example: Periodic Save

```rust
// User code - save every 5 minutes
async fn periodic_save(gateway: &GatewayState) -> Result<(), Error> {
    // Serialize quality history
    let history_json = serde_json::to_string(&gateway.hopper.quality_history)?;

    // Save to database
    sqlx::query!("UPDATE state SET quality_history = ?", history_json)
        .execute(&gateway.db).await?;

    Ok(())
}

// Set up timer
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        periodic_save(&gateway).await.ok();
    }
});
```

### Example: Shutdown Hook

```rust
// Save on graceful shutdown
async fn save_on_shutdown(gateway: Arc<GatewayState>) {
    tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");

    // Save all states
    let states = StateSnapshot {
        adr_metrics: gateway.adr.metrics_history.clone(),
        channel_quality: gateway.hopper.quality_history.clone(),
        device_stats: gateway.smart_decoder.device_stats.lock().clone(),
    };

    let json = serde_json::to_vec(&states).unwrap();
    tokio::fs::write("gateway_state.json", json).await.ok();

    println!("State saved. Shutting down...");
}
```

### Example: Startup Restore

```rust
// Restore on startup
async fn restore_state(gateway: &mut GatewayState) -> Result<(), Error> {
    if let Ok(json) = tokio::fs::read("gateway_state.json").await {
        let snapshot: StateSnapshot = serde_json::from_slice(&json)?;

        gateway.adr.metrics_history = snapshot.adr_metrics;
        gateway.hopper.quality_history = snapshot.channel_quality;
        *gateway.smart_decoder.device_stats.lock() = snapshot.device_stats;

        println!("State restored from previous session");
    }
    Ok(())
}
```

## Edge Cases and Considerations

### Restart Behavior

- **Lost State**: Recent quality metrics, channel blacklists, error counters
- **Recovery**: Gateway re-learns (e.g., scan channels, rebuild stats)
- **Mitigation**: Frequent snapshots (e.g., every minute for critical data)

### High Load Scenarios

- **1000+ Devices**: ~1-10MB total memory (still manageable)
- **Mitigation**: User sharding (e.g., per-region HashMap)
- **Eviction**: Automatic size limits prevent OOM

### Threading Considerations

- **Shared State**: `Arc<Mutex<T>>` for multi-thread access
- **Example**: `SmartDecoder::device_stats` shared between RX/TX threads
- **Performance**: User manages lock contention (e.g., batch updates)

## Best Practices

1. **Let States Be Transient**
   - Don't fight the design—embrace ephemeral state
   - Add persistence only where business-critical

2. **Use Eviction Policies**
   - Rely on built-in limits (e.g., 100 samples max)
   - Add manual resets for predictable behavior

3. **Snapshot Strategically**
   - Critical data: Every minute
   - Statistics: Every 5-15 minutes
   - Full state: On shutdown only

4. **Handle Restarts Gracefully**
   - Design for cold starts (no assumptions about state)
   - Use defaults that work without history

5. **Monitor Memory Usage**
   - Track HashMap sizes in production
   - Add alerts for unexpected growth

## Summary

The crate's transient state design provides:

- **Zero Lock-in**: Drop/recreate structs anytime
- **Predictable Memory**: Bounded by eviction policies
- **User Control**: Full ownership of persistence strategy
- **Clean Separation**: Business logic vs state management

This keeps the crate pure, testable, and flexible for any deployment scenario—from embedded devices to cloud gateways.
