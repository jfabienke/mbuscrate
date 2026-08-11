// RadioLib A/B reference receiver for wM-Bus mode C on the Waveshare SX1262 HAT.
//
// Mirrors the mbus-rs sx1262-rx probe: same frequency, bitrate, deviation,
// bandwidth, sync word and fixed-length/no-CRC/no-whitening framing, and the same
// output contract — one hex frame per line on stdout (replay-compatible), human
// heartbeat on stderr. Differences in behaviour between the two receivers are then
// attributable to the drivers, not the rigs.
//
// Deliberately NOT aligned: RadioLib derives its own preamble-detector length from
// the sync/preamble configuration (24 bits here, vs our 8). That is part of what is
// being compared.

#include <RadioLib.h>
#include "hal/RPi/PiHal.h"

#include <chrono>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <cstring>

static volatile sig_atomic_t stop_flag = 0;
static void on_sigint(int) { stop_flag = 1; }

static long long now_ms() {
  return std::chrono::duration_cast<std::chrono::milliseconds>(
             std::chrono::system_clock::now().time_since_epoch())
      .count();
}

int main(int argc, char** argv) {
  int seconds = (argc > 1) ? atoi(argv[1]) : 120;
  std::signal(SIGINT, on_sigint);

  // spidev0.1 keeps CE0 free; the HAT's NSS is plain GPIO21, toggled by RadioLib.
  PiHal* hal = new PiHal(1, 2000000, 0, 4);
  // Module(hal, cs, irq=DIO1, rst, gpio=BUSY) — Waveshare SX1262 XXXM pinout.
  SX1262 radio = new Module(hal, 21, 16, 18, 20);

  // XTAL board: TCXO voltage must be 0 or XOSC never starts (default is 1.6 V).
  int16_t state = radio.beginFSK(868.95, 100.0, 50.0, 234.3, 10, 32, 0.0, false);
  if (state != RADIOLIB_ERR_NONE) {
    fprintf(stderr, "beginFSK failed: %d\n", state);
    return 1;
  }

  uint8_t sync[] = {0x54, 0x3D, 0x54};
  state = radio.setSyncWord(sync, 3);
  state |= radio.fixedPacketLengthMode(255);
  state |= radio.setCRC(0);
  state |= radio.setWhitening(false);
  state |= radio.setRxBoostedGainMode(true, true);
  state |= radio.setDio2AsRfSwitch(true);
  if (state != RADIOLIB_ERR_NONE) {
    fprintf(stderr, "config failed: %d\n", state);
    return 1;
  }

  state = radio.startReceive();
  if (state != RADIOLIB_ERR_NONE) {
    fprintf(stderr, "startReceive failed: %d\n", state);
    return 1;
  }
  fprintf(stderr, "radiolib: listening %ds — 868.95 MHz, 100 kbps, sync 54 3D 54\n",
          seconds);

  long long t0 = now_ms();
  long long last_beat = t0;
  unsigned frames = 0;

  while (!stop_flag && (now_ms() - t0) < (long long)seconds * 1000) {
    uint32_t irq = radio.getIrqFlags();
    if (irq & RADIOLIB_SX126X_IRQ_RX_DONE) {
      uint8_t buf[255];
      int16_t st = radio.readData(buf, sizeof(buf));
      float rssi = radio.getRSSI();
      if (st == RADIOLIB_ERR_NONE || st == RADIOLIB_ERR_CRC_MISMATCH) {
        frames++;
        // stdout: bare hex, one frame per line (metermon-rs replay format).
        for (size_t i = 0; i < sizeof(buf); i++) printf("%02x", buf[i]);
        printf("\n");
        fflush(stdout);
        fprintf(stderr, "RX %lld rssi %.0f dBm\n", now_ms() - t0, rssi);
      } else {
        fprintf(stderr, "readData error: %d\n", st);
      }
      // Re-arm so the buffer pointer is reset for the next frame.
      radio.startReceive();
    }
    if (now_ms() - last_beat >= 15000) {
      fprintf(stderr, "  -- %llds · frames %u · rssi_now %.0f dBm\n",
              (now_ms() - t0) / 1000, frames, radio.getRSSI(false));
      last_beat = now_ms();
    }
    hal->delayMicroseconds(500);
  }

  fprintf(stderr, "radiolib: done, %u frames in %ds\n", frames, seconds);
  radio.standby();
  return 0;
}
