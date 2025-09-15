pub mod driver;
pub mod hal;
pub mod irq;
pub mod modulation;
pub mod radio_driver;

// PIO IRQ debouncing for Raspberry Pi 5
#[cfg(feature = "pio-irq")]
pub mod pio_irq;

// LoRa support
pub mod lora;

// RFM69 packet processing (always available for testing)
pub mod rfm69_packet;
pub mod rfm69_registers;

// RFM69 driver (feature-gated for hardware)
#[cfg(feature = "rfm69")]
pub mod rfm69;
