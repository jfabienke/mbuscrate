# sx1262.rs reconciliation plan (read-only)

Status: **analysis only — no code changes proposed here.** This maps the boundary between the
quarantined `sx1262.rs` demo, the real `RaspberryPiHal`, the PIO IRQ backend, and the canonical
`Sx126xDriver`, so implementation starts only once the open questions below are answered.

## 1. What `sx1262.rs` actually is

`src/wmbus/radio/lora/sx1262.rs` (behind the `pio-irq` feature, `#[deprecated]` since `fee28dd`)
is a demo that conflates **two separable concerns**:

1. **A mock SPI layer.** `spi_command` / `spi_command_with_response` do no I/O — they `debug!` +
   `thread::sleep` and return canned bytes. Its `configure_lora`, `read_packet`,
   `transmit_packet`, `get_irq_status`, etc. all ride on this mock, so **none of it talks to
   hardware.**
2. **A PIO IRQ integration.** `is_packet_ready()` / `wait_tx_done()` call
   `PioIrqBackend::debounce_irq(DIO1_RX_DONE / DIO0_TX_DONE, …)` and then confirm via a
   (mock) SPI `GetIrqStatus` read. This — hardware-debounced DIO-edge detection — is the only
   part with real value; it exists nowhere else in the crate's *live* path.

## 2. The three boundaries, mapped

### 2a. Canonical `Sx126xDriver<H>` (the real driver)
- Generic over the `Hal` trait (SPI command/register + GPIO). Does **all** IRQ handling by
  **SPI polling**: `process_irqs()` issues `GetIrqStatus` (0x12), reads the buffer, etc.
- The `WMBusHandle` receiver loop arms continuous RX once, then `sleep(10ms)` +
  `process_irqs_with_mode()` — **it never reads a DIO GPIO edge.**
- Real config is `RadioProfile` + `switch_profile`; real reception is `process_irqs_with_mode`.

### 2b. `RaspberryPiHal` (`hal/raspberry_pi.rs`, feature `raspberry-pi = ["rppal"]`)
- `impl Hal for RaspberryPiHal` provides **real SPI** (`rppal::spi::Spi`) and GPIO. This is the
  production path today: `WMBusHandleFactory::create_raspberry_pi_*` builds
  `Sx126xDriver<RaspberryPiHal>`.
- `GpioPins { busy, dio1: 24, dio2: Some(23), reset }` — it models **DIO1 on GPIO24, DIO2 on
  GPIO23**, driven through rppal. The canonical driver uses BUSY for readiness; DIO edges are
  **not** used for IRQ (that's SPI-polled).

### 2c. PIO IRQ backend (`radio/pio_irq.rs`, feature `pio-irq = ["dep:nix", "dep:libc"]`)
- `trait PioIrqBackend { debounce_irq, clear_irq_fifo, is_irq_pending }`; global singleton via
  `get_pio_irq_backend()`. Two impls: `PioIrqHardwareBackend` (RP1 PIO, `aarch64`+`linux`,
  nix/libc) and a cross-platform `SoftwareBackend` fallback (no real acceleration off-Pi5).
- Models **DIO_PINS = [25,26,27,28]; DIO0=GPIO25=TX_DONE, DIO1=GPIO26=RX_DONE**, claimed
  through **nix/libc**, not rppal.
- `pio-irq` does **not** depend on `raspberry-pi`/`rppal`; the two feature sets are independent.

## 3. Boundary problems (why this isn't a drop-in)

1. **DIO pin mismatch.** `RaspberryPiHal` says DIO1=GPIO24; the PIO backend says DIO1=GPIO26
   (and DIO0=GPIO25). These describe **different wirings** — they cannot both be right for one
   board. This must be resolved against the actual HAT before any integration.
2. **GPIO ownership contention.** `RaspberryPiHal` claims GPIO via **rppal**; the PIO backend
   claims GPIO25-28 via **nix/libc**. Two libraries owning overlapping pins is a runtime
   conflict; the design must decide a single owner for the DIO lines.
3. **Feature composition.** A real PIO-accelerated Pi build needs **both** `pio-irq` and
   `raspberry-pi` enabled together; no code currently composes them (they're orthogonal today).
4. **Redundant IRQ landscape.** PIO is one of several unused IRQ-acceleration modules
   (`hal/enhanced_gpio.rs`, `radio/pio_irq.rs`, `lora/irq_queue.rs`), none wired to the
   SPI-polled receive loop. Picking PIO means explicitly *not* the others.

## 4. Two reconciliation paths

### Path A — Retire `sx1262.rs`; keep `pio_irq.rs` (recommended now)
The mock driver's SPI half is pure redundancy (`RaspberryPiHal` does real SPI) and its config
half duplicates `RadioProfile`. Its only real asset (`pio_irq.rs`) is a **separate module** that
survives independently.
- **Delete** `sx1262.rs` (the `Sx1262Driver` demo, `LoRaConfig`, mock SPI) and its `pio-irq`
  re-export in `lora/mod.rs`.
- **Keep** `radio/pio_irq.rs` (the backends + their tests) untouched, as the substrate for a
  future Path B.
- **Resolve** the `examples/pio_irq_demo.rs` + `tests/pio_irq_tests.rs` bit-rot: either delete
  them or re-point them at `get_pio_irq_backend()` directly (they exercise the backend, not the
  mock driver). Note both have pre-existing missing-import breakage today.
- Result: the **divergent profile path is gone**; the canonical `Sx126xDriver<RaspberryPiHal>` is
  the single implementation. No fake HAL invented. Small, reviewable, no hardware needed.

### Path B — Wire PIO IRQ into the canonical receive loop (scoped future work)
Preserve the PIO value as an **enhancement to the existing receive loop**, not a driver.
- The receiver, instead of `sleep(10ms)`, waits on the PIO-debounced **RX_DONE** edge, then calls
  the existing `process_irqs_with_mode()` (SPI) to confirm + read the packet — exactly the
  `is_packet_ready` pattern, but on the canonical driver.
- Shape: an optional `IrqSource` the handle can be built with (default = current 10ms poll; Pi =
  a `PioIrqBackend`-backed source), gated on `pio-irq` **and** `raspberry-pi`.
- **Blocked on the open questions below** — do not start until they're answered (per the "clear
  boundary first" instruction). This is where a real PIO/HAL adapter belongs; it is an IRQ-wait
  source, not a second driver, and needs **no** invented HAL.

## 5. Open questions to resolve before Path B

1. **Actual DIO wiring**: which GPIOs are DIO0/DIO1/DIO2 on the target HAT — GPIO24/23
   (`RaspberryPiHal`) or GPIO25-28 (`pio_irq`)? One must be corrected to match hardware.
2. **GPIO owner**: does rppal (`RaspberryPiHal`) or nix/libc (PIO) own the DIO lines? Can they
   coexist, or must the PIO backend read pins the HAL has already claimed?
3. **Scope of acceleration**: RX_DONE only, or TX_DONE too? (The receive-latency win is RX_DONE.)
4. **Fallback semantics**: off-Pi5 the PIO `SoftwareBackend` gives no real acceleration — should
   the handle silently fall back to 10ms polling there, or refuse the PIO source?

## 6. Recommendation

Do **Path A** now (delete the mock, keep `pio_irq.rs`, fix/retire its example+test) — it removes
the last divergent profile path with zero hardware risk. Treat **Path B** as a separate,
hardware-gated feature to open only after the §5 questions (especially the DIO pin/ownership
conflict) are answered on real hardware.
