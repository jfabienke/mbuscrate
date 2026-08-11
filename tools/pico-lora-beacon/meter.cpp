// LoRaWAN meter emulator — a Zenner-like water meter on a Pico-LoRa-SX126X.
//
// Joins by OTAA using RadioLib's LoRaWAN stack (an implementation independent of
// mbus-rs, so a successful join validates our responder rather than agreeing with
// it), then uplinks meter readings as **OMS/wM-Bus data records**. That payload
// choice is deliberate: the gateway decodes it with the record parser it already
// uses for wired and wireless M-Bus, so the transport changes and the decode path
// does not.
//
// Honesty about the data, because a test rig whose output is indistinguishable
// from a real meter's is a liability:
//
//   volume       SIMULATED  — there is no flow sensor on this board
//   temperature  REAL       — RP2040 on-die sensor. It measures the *chip*, which
//                             runs several degrees above ambient and warms further
//                             with radio activity. It is not a water temperature.
//   battery      REAL       — VSYS/3 through ADC3, the actual supply rail
//
// Build: see README.md. Credentials come from -DMETER_DEV_EUI etc. at configure
// time so no key is ever committed.

#include <hardware/adc.h>
#include <hardware/gpio.h>
#include <pico/stdlib.h>

#include <cstdio>
#include <cstring>

#include <RadioLib.h>
#include "hal/RPiPico/PicoHal.h"

#define SPI_PORT spi1
#define PIN_SCK  10
#define PIN_MOSI 11
#define PIN_MISO 12
#define PIN_NSS   3
#define PIN_BUSY  2
#define PIN_DIO1 20
#define PIN_RST  15

// This board's Core1262 has a TCXO on DIO3; see README. With 0.0 the chip fails
// begin() as -707 with XOSC_START_ERR.
static const float TCXO_V = 1.7f;

// Credentials, overridable at configure time. These defaults are obviously fake
// so an unconfigured build cannot be mistaken for a provisioned device.
#ifndef METER_JOIN_EUI
#define METER_JOIN_EUI 0x0000000000000000
#endif
#ifndef METER_DEV_EUI
#define METER_DEV_EUI 0x0004A30B00FF0001
#endif
#ifndef METER_APP_KEY
#define METER_APP_KEY 0x2B, 0x7E, 0x15, 0x16, 0x28, 0xAE, 0xD2, 0xA6, \
                      0xAB, 0xF7, 0x15, 0x88, 0x09, 0xCF, 0x4F, 0x3C
#endif

static const uint32_t UPLINK_PERIOD_MS = 60000;
static const uint8_t  METER_FPORT = 1;

/// RP2040 on-die temperature, in tenths of a degree Celsius.
/// Conversion is the datasheet's: T = 27 - (V - 0.706) / 0.001721.
static int16_t read_die_temp_decidegrees() {
  adc_select_input(4);
  const float conv = 3.3f / (1 << 12);
  float v = adc_read() * conv;
  float t = 27.0f - (v - 0.706f) / 0.001721f;
  return (int16_t)(t * 10.0f);
}

/// Supply rail in millivolts, or -1 when the reading is not credible.
///
/// VSYS reaches ADC3 through a 3:1 divider. On the Pico W that pin is shared with
/// the wireless SPI, so the reading is sanity-checked rather than trusted: a
/// plausible-but-wrong voltage published as meter telemetry is worse than an
/// absent one, and this project has already lost hours to a believable constant
/// that turned out to be a status register.
static int32_t read_vsys_mv() {
  gpio_init(25);
  gpio_set_dir(25, GPIO_OUT);
  gpio_put(25, 1); // release the shared pin on Pico W
  sleep_ms(2);
  adc_select_input(3);
  const float conv = 3.3f / (1 << 12);
  int32_t mv = (int32_t)(adc_read() * conv * 3.0f * 1000.0f);
  // USB is ~5 V, a LiPo 3.0-4.3 V. Anything outside that is not a supply rail.
  return (mv > 2500 && mv < 5600) ? mv : -1;
}

/// Append an 8-digit BCD volume record: DIF 0x0C, VIF 0x13 (volume, 10^-3 m3).
static size_t put_volume(uint8_t* p, uint32_t litres) {
  size_t n = 0;
  p[n++] = 0x0C; // 8-digit BCD
  p[n++] = 0x13; // volume, litres
  for (int i = 0; i < 4; i++) {
    uint8_t lo = litres % 10; litres /= 10;
    uint8_t hi = litres % 10; litres /= 10;
    p[n++] = (uint8_t)((hi << 4) | lo); // BCD is little-endian, low digits first
  }
  return n;
}

/// Append a 16-bit temperature record: DIF 0x02, VIF 0x66 (external temp, 10^-1 C).
/// External temperature is the exponent range 0x64-0x67 = 10^(nn-3) C, so tenths is
/// 0x66 — 0x65 is hundredths, and using it made 23.8 C decode as 2.38 C.
static size_t put_temperature(uint8_t* p, int16_t decidegrees) {
  size_t n = 0;
  p[n++] = 0x02;
  p[n++] = 0x66;
  p[n++] = (uint8_t)(decidegrees & 0xFF);
  p[n++] = (uint8_t)((decidegrees >> 8) & 0xFF);
  return n;
}

/// Append a 16-bit voltage record: DIF 0x02, VIF 0xFD (extension) VIFE 0x46
/// (voltage, 10^(6-9) V = millivolts). Exercises the VIF-extension path.
static size_t put_battery(uint8_t* p, int16_t millivolts) {
  size_t n = 0;
  p[n++] = 0x02;
  p[n++] = 0xFD;
  p[n++] = 0x46;
  p[n++] = (uint8_t)(millivolts & 0xFF);
  p[n++] = (uint8_t)((millivolts >> 8) & 0xFF);
  return n;
}

int main() {
  stdio_init_all();
  sleep_ms(2000);
  adc_init();
  adc_set_temp_sensor_enabled(true);

  PicoHal* hal = new PicoHal(SPI_PORT, PIN_MISO, PIN_MOSI, PIN_SCK);
  SX1262 radio = new Module(hal, PIN_NSS, PIN_DIO1, PIN_RST, PIN_BUSY);
  LoRaWANNode node(&radio, &EU868);

  printf("\n[meter] LoRaWAN water-meter emulator (OMS records over LoRaWAN)\n");

  // LoRaWANNode overrides most of this per data rate; the values that matter here
  // are the TCXO voltage and that the radio comes up at all.
  int state = radio.begin(868.1, 125.0, 9, 5, RADIOLIB_SX126X_SYNC_WORD_PRIVATE,
                          10, 8, TCXO_V, false);
  if (state != RADIOLIB_ERR_NONE) {
    printf("[meter] radio begin failed: %d\n", state);
    while (true) sleep_ms(3000);
  }
  radio.setDio2AsRfSwitch(true);

  const uint8_t appKey[] = {METER_APP_KEY};
  // nwkKey = NULL selects LoRaWAN 1.0.x, which is what the fleet's hardware runs.
  state = node.beginOTAA(METER_JOIN_EUI, METER_DEV_EUI, NULL, (uint8_t*)appKey);
  if (state != RADIOLIB_ERR_NONE) {
    printf("[meter] beginOTAA failed: %d\n", state);
    while (true) sleep_ms(3000);
  }

  printf("[meter] joining (OTAA)...\n");
  while (true) {
    state = node.activateOTAA();
    if (state == RADIOLIB_LORAWAN_NEW_SESSION || state == RADIOLIB_LORAWAN_SESSION_RESTORED) {
      printf("[meter] JOINED (state %d)\n", state);
      break;
    }
    // The gateway listens on one channel; EU868 picks randomly among three join
    // channels, so a few attempts are expected before one lands.
    printf("[meter] join attempt failed: %d — retrying\n", state);
    sleep_ms(15000);
  }

  // EU868 joins at DR3 (SF9) — the default data rate of every join channel. Staying
  // there keeps uplinks on the same spreading factor the gateway is parked on; a
  // single-channel, single-SF listener cannot follow a device that moves.
  node.setDatarate(3);

  uint32_t litres = 1234567;
  uint32_t n = 0;
  while (true) {
    litres += 3 + (n % 7); // a plausible trickle; SIMULATED

    uint8_t payload[32];
    size_t len = 0;
    len += put_volume(payload + len, litres);
    int16_t temp = read_die_temp_decidegrees();
    len += put_temperature(payload + len, temp);
    int32_t mv = read_vsys_mv();
    if (mv > 0) {
      len += put_battery(payload + len, (int16_t)mv);
    }

    printf("[meter] uplink %lu: %lu L, %.1f C (die), %ld mV — %u B:", ++n, litres,
           temp / 10.0f, (long)mv, (unsigned)len);
    for (size_t i = 0; i < len; i++) printf(" %02X", payload[i]);
    printf("\n");

    state = node.sendReceive(payload, len, METER_FPORT);
    printf("[meter]   sendReceive -> %d\n", state);
    sleep_ms(UPLINK_PERIOD_MS);
  }
}
