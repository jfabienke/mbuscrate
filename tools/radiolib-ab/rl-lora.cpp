// RadioLib LoRa reference receiver — the LoRa twin of rlrx.cpp.
//
// Arbiter for the crate's LoRa receive path: same chip, same antenna, same
// (frequency, SF) schedule, driven by RadioLib (MIT). If this hears a device our
// driver doesn't, the driver is wrong; if both are silent, the traffic isn't
// there. Sweeps the EU868 join channels across the SF ladder, LoRaWAN public
// sync word, explicit header, CRC on, standard IQ — the uplink configuration.
//
// Build: see README.md. Run: ./rl-lora <total-seconds> <dwell-seconds>

#include <RadioLib.h>
#include "hal/RPi/PiHal.h"

#include <chrono>
#include <csignal>
#include <cstdio>
#include <cstdlib>

static volatile sig_atomic_t stop_flag = 0;
static void on_sigint(int) { stop_flag = 1; }

static long long now_ms() {
  return std::chrono::duration_cast<std::chrono::milliseconds>(
             std::chrono::system_clock::now().time_since_epoch())
      .count();
}

int main(int argc, char** argv) {
  int seconds = (argc > 1) ? atoi(argv[1]) : 360;
  int dwell = (argc > 2) ? atoi(argv[2]) : 20;
  // Optional: park on one frequency and sync word to hunt a bench transmitter
  // instead of sweeping the LoRaWAN join plan.
  float fixed_freq = (argc > 3) ? atof(argv[3]) : 0.0f;
  int syncbyte = (argc > 4) ? (int)strtol(argv[4], NULL, 0) : 0x34;
  std::signal(SIGINT, on_sigint);

  // Pi 5 (kernel < 6.6.45): header GPIOs live on gpiochip4.
  PiHal* hal = new PiHal(1, 2000000, 0, 4);
  // Module(hal, cs, irq=DIO1, rst, gpio=BUSY) — Waveshare SX1262 XXXM pinout.
  SX1262 radio = new Module(hal, 21, 16, 18, 20);

  // XTAL board: TCXO voltage 0. Sync 0x34 = public LoRaWAN (expands to 0x3444).
  float f0 = fixed_freq > 0 ? fixed_freq : 868.1f;
  int16_t state = radio.begin(f0, 125.0, 12, 5, (uint8_t)syncbyte, 10, 8, 0.0, false);
  if (state != RADIOLIB_ERR_NONE) {
    fprintf(stderr, "begin failed: %d\n", state);
    return 1;
  }
  // A bench transmitter a metre away arrives near -24 dBm, which compresses the
  // front end; boosted gain only makes that worse.
  radio.setRxBoostedGainMode(fixed_freq > 0 ? false : true, true);
  radio.setDio2AsRfSwitch(true);

  const float join_freqs[] = {868.1f, 868.3f, 868.5f};
  const float bench_freqs[] = {0, 0, 0}; // filled from fixed_freq below
  (void)bench_freqs;
  const int sfs[] = {12, 9, 7, 10, 8, 11};
  const float bws[] = {125.0f, 250.0f, 500.0f};
  fprintf(stderr,
          "rl-lora: %ds total, %ds dwell, sync 0x%02X, %s\n", seconds, dwell,
          syncbyte,
          fixed_freq > 0 ? "bench mode (fixed freq, SF x BW sweep)"
                         : "join channels x SF ladder");

  long long t0 = now_ms();
  unsigned frames = 0;
  int point = 0;

  while (!stop_flag && (now_ms() - t0) < (long long)seconds * 1000) {
    float f, bw;
    int sf;
    if (fixed_freq > 0) {
      f = fixed_freq;
      sf = sfs[point % 6];
      bw = bws[(point / 6) % 3];
    } else {
      f = join_freqs[point % 3];
      sf = sfs[(point / 3) % 6];
      bw = 125.0f;
    }
    point++;
    radio.standby();
    radio.setFrequency(f);
    radio.setBandwidth(bw);
    radio.setSpreadingFactor(sf);
    state = radio.startReceive();
    if (state != RADIOLIB_ERR_NONE) {
      fprintf(stderr, "startReceive failed: %d\n", state);
      return 1;
    }
    fprintf(stderr, "-- %llds listening %.3f MHz SF%d BW%.0f\n",
            (now_ms() - t0) / 1000, f, sf, bw);

    long long dwell_end = now_ms() + (long long)dwell * 1000;
    while (!stop_flag && now_ms() < dwell_end &&
           (now_ms() - t0) < (long long)seconds * 1000) {
      uint32_t irq = radio.getIrqFlags();
      if (irq & RADIOLIB_SX126X_IRQ_RX_DONE) {
        uint8_t buf[255] = {0};
        size_t len = radio.getPacketLength();
        int16_t st = radio.readData(buf, len);
        if (st == RADIOLIB_ERR_NONE || st == RADIOLIB_ERR_CRC_MISMATCH) {
          frames++;
          printf("RX %3zuB rssi %.0f dBm snr %.1f dB %.3f MHz SF%d BW%.0f%s  ", len,
                 radio.getRSSI(), radio.getSNR(), f, sf, bw,
                 st == RADIOLIB_ERR_CRC_MISMATCH ? " CRC-ERR" : "");
          for (size_t i = 0; i < len; i++) printf("%02x", buf[i]);
          printf("\n");
          fflush(stdout);
        }
        radio.startReceive();
      }
      hal->delayMicroseconds(1000);
    }
  }

  fprintf(stderr, "rl-lora: done, %u frames in %llds\n", frames,
          (now_ms() - t0) / 1000);
  radio.standby();
  return 0;
}
