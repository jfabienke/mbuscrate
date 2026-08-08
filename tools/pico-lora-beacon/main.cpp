// LoRa beacon for the Waveshare Pico-LoRa-SX126X — an independent reference
// transmitter for the gateway's LoRa receive path.
//
// The Ebyte E22 could not serve this role: its air-rate presets map to an
// undocumented (SF, BW) pair and its sync word is unpublished, so neither our
// driver nor RadioLib could decode it and the failure was uninterpretable. This
// board carries a Core1262-HF — the same module as the gateway HAT — on SPI, so
// every LoRa parameter is set explicitly here and matched explicitly there. If
// the gateway then hears nothing, the fault is in the gateway.
//
// RadioLib (MIT) drives it deliberately: an oracle built on our own driver would
// share our own bugs and prove nothing.

#include <pico/stdlib.h>
#include <cstdio>

#include <RadioLib.h>
#include "hal/RPiPico/PicoHal.h"

// Waveshare Pico-LoRa-SX126X pinout. Treated as a hypothesis to be confirmed by
// the chip answering over SPI, not as fact — the gateway HAT's NSS turned out to
// be a bit-banged GPIO rather than a hardware chip-select.
#define SPI_PORT spi1
#define PIN_SCK  10
#define PIN_MOSI 11
#define PIN_MISO 12
#define PIN_NSS   3
#define PIN_BUSY  2
#define PIN_DIO1 20
#define PIN_RST  15

// The contract with the receiver. Every value is stated rather than defaulted,
// because a LoRa link is deaf on any single mismatch and gives no error saying so.
static const float   FREQ_MHZ   = 868.1f;
static const float   BW_KHZ     = 125.0f;
static const uint8_t SF         = 7;
static const uint8_t CR         = 5;      // 4/5
static const uint8_t SYNC_WORD  = 0x12;   // private LoRa (register 0x1424)
static const int8_t  POWER_DBM  = 2;      // bench range: keep the receiver out of compression
static const uint16_t PREAMBLE  = 8;
// Whether this board's Core1262 runs a plain crystal or a TCXO on DIO3 is not
// something to guess: Waveshare's own module and board schematics disagree, and
// getting it wrong fails as SPI_CMD_FAILED (-707) during calibration rather than
// as anything mentioning oscillators. So try each candidate and report which
// works, instead of burning a flash cycle per attempt.
static const float TCXO_CANDIDATES[] = {0.0f, 1.7f, 1.8f, 2.4f, 3.0f, 3.3f};
static const uint32_t PERIOD_MS = 2000;

// getDeviceErrors()/clearDeviceErrors() are protected in RadioLib. Exposing them
// is worth a subclass: the SX126x reports XOSC_START_ERR when the oscillator
// configuration is wrong, which turns an opaque -707 into a specific diagnosis.
class SX1262Diag : public SX1262 {
 public:
  explicit SX1262Diag(Module* m) : SX1262(m) {}
  uint16_t devErrors() { return getDeviceErrors(); }
  int16_t clearDevErrors() { return clearDeviceErrors(); }
};

int main() {
  stdio_init_all();
  sleep_ms(2000); // let USB CDC attach so the first lines are not lost

  PicoHal* hal = new PicoHal(SPI_PORT, PIN_MISO, PIN_MOSI, PIN_SCK);
  // Module(hal, cs, irq, rst, gpio) — gpio is BUSY on the SX126x.
  SX1262Diag radio(new Module(hal, PIN_NSS, PIN_DIO1, PIN_RST, PIN_BUSY));

  printf("\n[beacon] %.1f MHz SF%u BW%.0f CR4/%u sync 0x%02X %d dBm\n",
         FREQ_MHZ, SF, BW_KHZ, CR, SYNC_WORD, POWER_DBM);

  int state = RADIOLIB_ERR_UNKNOWN;
  float tcxo_used = -1.0f;
  for (float v : TCXO_CANDIDATES) {
    state = radio.begin(FREQ_MHZ, BW_KHZ, SF, CR, SYNC_WORD, POWER_DBM,
                        PREAMBLE, v, false);
    uint16_t errs = radio.devErrors();
    printf("[beacon] begin(tcxo=%.1f V) -> %d  device_errors=0x%04X%s\n", v, state,
           errs, (errs & 0x0020) ? "  XOSC_START_ERR" : "");
    if (state == RADIOLIB_ERR_NONE) { tcxo_used = v; break; }
    radio.clearDevErrors();
    sleep_ms(200);
  }
  if (state != RADIOLIB_ERR_NONE) {
    // -2 CHIP_NOT_FOUND would mean the pinout is wrong; -707 SPI_CMD_FAILED means
    // the chip answers but a command failed, which is an oscillator/calibration
    // problem rather than a wiring one.
    while (true) {
      printf("[beacon] no working configuration; last state %d%s\n", state,
             state == -2 ? "  (chip not found - check the pinout)" : "");
      sleep_ms(3000);
    }
  }
  printf("[beacon] radio up with tcxo=%.1f V\n", tcxo_used);

  // The board ties its antenna switch to DIO2; without this the PA drives a
  // disconnected port.
  radio.setDio2AsRfSwitch(true);
  radio.explicitHeader();
  radio.setCRC(2);

  printf("[beacon] radio ready, transmitting every %lu ms\n", PERIOD_MS);

  uint32_t n = 0;
  while (true) {
    char msg[32];
    snprintf(msg, sizeof(msg), "PICO-BEACON-%05lu", ++n);
    int st = radio.transmit(msg);
    printf("[beacon] %s -> %s\n", msg,
           st == RADIOLIB_ERR_NONE ? "ok" : "FAILED");
    sleep_ms(PERIOD_MS);
  }
}
