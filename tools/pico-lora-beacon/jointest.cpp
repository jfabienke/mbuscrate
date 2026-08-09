// LoRaWAN join-persistence hardware tester for the Waveshare Pico-LoRa-SX126X.
//
// Drives the metermon-rs join responder to validate the 1.0.4 anti-replay
// persistence work (docs/design/lorawan-join-persistence.md). RadioLib's LoRaWAN
// stack is an implementation independent of mbus-rs, so a join it accepts
// validates our responder rather than agreeing with it.
//
// Command-driven over the USB console so joins can be timed against a responder
// restart:
//   j : perform one OTAA join. clearSession() first so every 'j' is a fresh
//       JoinRequest carrying the next DevNonce; retries across the random join
//       channel until the single-channel responder (868.1 MHz, SF9) hears it.
//       Prints the DevNonce sent and the JoinNonce received.
//   s : print whether a session is currently active.
//
// Test (a) — JoinNonce monotonicity across a responder restart:
//   join (note JoinNonce N) -> restart the responder -> join again *without
//   rebooting this board* (so RadioLib's DevNonce keeps climbing). The second
//   JoinNonce must be > N and the join must still succeed — proving the responder
//   persisted its JoinNonce rather than resetting it to 1.
//
// Test (b) — DevNonce replay protection:
//   power-cycle this board. RadioLib's DevNonce counter lives in RAM, so it
//   resets low. Every subsequent JoinRequest then carries a DevNonce at or below
//   the responder's stored high-water mark, so the responder rejects them all as
//   replays and the join never completes — proving the DevNonce store survived.
//
// Board facts (see README.md): Core1262-HF on SPI1, TCXO on DIO3 at 1.7 V,
// antenna switch on DIO2. Pins confirmed by the chip answering over SPI.

#include <pico/stdlib.h>

#include <cstdio>

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

// This board's Core1262 has a TCXO on DIO3; with 0.0 begin() fails as -707
// (XOSC_START_ERR). See README.md.
static const float TCXO_V = 1.7f;

// Credentials, overridable at configure time; the defaults match meter.cpp and
// the gateway's join-creds.json. The AppKey is the published LoRaWAN test vector
// (RFC/Semtech example), not a device secret — safe to keep in source.
#ifndef JT_JOIN_EUI
#define JT_JOIN_EUI 0x0000000000000000
#endif
#ifndef JT_DEV_EUI
#define JT_DEV_EUI 0x0004A30B00FF0001
#endif
#ifndef JT_APP_KEY
#define JT_APP_KEY 0x2B, 0x7E, 0x15, 0x16, 0x28, 0xAE, 0xD2, 0xA6, \
                   0xAB, 0xF7, 0x15, 0x88, 0x09, 0xCF, 0x4F, 0x3C
#endif

// EU868 picks one of three join channels at random; the responder watches one.
// Retry enough that a channel match is near-certain, but bounded so a genuinely
// rejecting responder (test b) reports a clean no-join instead of looping.
static const int      JOIN_ATTEMPTS  = 12;
static const uint32_t ATTEMPT_GAP_MS = 3000;

static void do_join(LoRaWANNode& node) {
  printf("[jt] join: starting (up to %d attempts on the random join channel)\n",
         JOIN_ATTEMPTS);
  // Force a fresh JoinRequest. clearSession() drops the session but keeps the
  // nonce buffer, so DevNonce keeps climbing across joins as 1.0.4 requires.
  node.clearSession();
  for (int i = 1; i <= JOIN_ATTEMPTS; i++) {
    LoRaWANJoinEvent_t ev;
    int st = node.activateOTAA(&ev);
    bool joined = (st == RADIOLIB_LORAWAN_NEW_SESSION);
    printf("[jt]   attempt %d: state=%d devNonce=%u joinNonce=%lu%s\n", i, st,
           ev.devNonce, (unsigned long)ev.joinNonce, joined ? "  JOINED" : "");
    if (joined) {
      printf("[jt] RESULT joined  devNonce=%u  joinNonce=%lu\n", ev.devNonce,
             (unsigned long)ev.joinNonce);
      return;
    }
    sleep_ms(ATTEMPT_GAP_MS);
  }
  printf("[jt] RESULT no-join after %d attempts "
         "(responder silent, or rejecting every DevNonce as a replay)\n",
         JOIN_ATTEMPTS);
}

int main() {
  stdio_init_all();
  sleep_ms(2000);  // let USB CDC attach so the first lines are not lost

  PicoHal* hal = new PicoHal(SPI_PORT, PIN_MISO, PIN_MOSI, PIN_SCK);
  SX1262 radio = new Module(hal, PIN_NSS, PIN_DIO1, PIN_RST, PIN_BUSY);
  LoRaWANNode node(&radio, &EU868);

  printf("\n[jt] LoRaWAN join-persistence tester\n");

  // LoRaWANNode overrides most air parameters per data rate; what matters here is
  // the TCXO voltage and that the radio answers at all.
  int state = radio.begin(868.1, 125.0, 9, 5, RADIOLIB_SX126X_SYNC_WORD_PRIVATE,
                          10, 8, TCXO_V, false);
  if (state != RADIOLIB_ERR_NONE) {
    printf("[jt] radio begin failed: %d\n", state);
    while (true) sleep_ms(3000);
  }
  radio.setDio2AsRfSwitch(true);

  const uint8_t appKey[] = {JT_APP_KEY};
  // nwkKey = NULL selects LoRaWAN 1.0.x, matching the responder and the fleet.
  state = node.beginOTAA(JT_JOIN_EUI, JT_DEV_EUI, NULL, (uint8_t*)appKey);
  if (state != RADIOLIB_ERR_NONE) {
    printf("[jt] beginOTAA failed: %d\n", state);
    while (true) sleep_ms(3000);
  }
  // Bench test against a co-located gateway: do not let the band's 1% duty cycle
  // stall the rapid retries this test issues.
  node.setDutyCycle(false);

  printf("[jt] ready. DevEUI=%016llX  —  send 'j' to join, 's' for session state\n",
         (unsigned long long)JT_DEV_EUI);

  absolute_time_t beat = make_timeout_time_ms(8000);
  while (true) {
    int c = getchar_timeout_us(0);
    if (c == 'j' || c == 'J') {
      do_join(node);
    } else if (c == 's' || c == 'S') {
      printf("[jt] session active=%d\n", node.isActivated());
    }
    if (time_reached(beat)) {
      printf("[jt] idle · send 'j' to join\n");
      beat = make_timeout_time_ms(15000);
    }
    sleep_ms(20);
  }
}
