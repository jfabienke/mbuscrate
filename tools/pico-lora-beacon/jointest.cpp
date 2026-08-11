// LoRaWAN join hardware tester for the Waveshare Pico-LoRa-SX126X.
//
// Drives the metermon-rs join responder. RadioLib's LoRaWAN stack is an
// implementation independent of mbus-rs, so a join it accepts validates our
// responder rather than agreeing with it.
//
// Command-driven over the USB console so joins can be timed against a responder
// restart:
//   j : join with RadioLib's own (monotonic) DevNonce. clearSession() first so
//       every 'j' is a fresh JoinRequest; retries across the random join channel
//       until the single-channel responder (868.1 MHz, SF9) hears it.
//   r : join with a *random* DevNonce (see below). The 1.0.2 emulation.
//   p : replay the last DevNonce used by 'r' — must be rejected by a correct
//       responder (a genuine replay), and is the deterministic PASS-3 check.
//   s : print whether a session is currently active.
//
// Why 'r' exists — RadioLib cannot emit random DevNonces. It increments DevNonce
// unconditionally, even with nwkKey=NULL (1.0.x) crypto, so a plain 'j' campaign
// only ever exercises the *counter* (1.0.4) anti-replay model. The real fleet runs
// LoRaWAN 1.0.2 on an LMIC stack that draws DevNonce randomly (os_getRndU2), which
// a strict high-water responder would reject on ~half of re-joins. 'r' injects a
// chosen DevNonce into RadioLib's signed nonces buffer (patch offset 7, re-sign
// with checkSum16 over the first 12 bytes at offset 12; layout asserted below) so
// this board faithfully stands in for a 1.0.2 meter. Each attempt re-injects the
// same nonce, and the sent DevNonce is checked against the injected one so a wrong
// offset/endianness aborts loudly instead of misleading the test.
//
// Test (a) — JoinNonce monotonicity across a responder restart:
//   join (note JoinNonce N) -> restart the responder -> join again *without
//   rebooting this board*. The second JoinNonce must be > N and the join must
//   still succeed — proving the responder persisted its JoinNonce.
//
// Test (b) — DevNonce replay protection (1.0.2 model): send several 'r' joins;
//   all must succeed (the windowed responder admits fresh random nonces). Then 'p'
//   replays one — it must be rejected. See docs/LORA_JOIN_REHEARSAL.md.
//
// Board facts (see README.md): Core1262-HF on SPI1, TCXO on DIO3 at 1.7 V,
// antenna switch on DIO2. Pins confirmed by the chip answering over SPI.

#include <pico/stdlib.h>
#include <pico/rand.h>

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

// Exposes LoRaWANNode's protected static helpers (checkSum16, hton) so the nonce
// buffer can be re-signed after patching DevNonce — same expose-a-protected-static
// pattern as SX1262Diag. Layout facts (RadioLib 7.7.1, LoRaWAN.h "SchemeBase"):
// buffer is RADIOLIB_LORAWAN_NONCES_BUF_SIZE = 14 bytes; DevNonce is a little-
// endian u16 at offset RADIOLIB_LORAWAN_NONCES_DEV_NONCE = 7; the signature is
// checkSum16 over the first BUF_SIZE-2 = 12 bytes, stored little-endian at offset
// RADIOLIB_LORAWAN_NONCES_SIGNATURE = 12. getBufferNonces() freshly signs and
// returns the node's own buffer, so patching *that* buffer keeps the MODE/PLAN/
// keyCheckSum fields consistent — setBufferNonces() rejects a buffer whose
// mode/plan/keys differ from the live node (RADIOLIB_ERR_NONCES_DISCARDED).
struct NonceTool : public LoRaWANNode {
  using LoRaWANNode::LoRaWANNode;

  // Inject `nonce` as the next DevNonce. Only valid with no active session
  // (setBufferNonces silently no-ops otherwise — hence the isActivated() guard).
  // Returns true on success.
  bool injectDevNonce(uint16_t nonce) {
    if (this->isActivated()) {
      printf("[jt] inject: refused — session active (clearSession first)\n");
      return false;
    }
    uint8_t buf[RADIOLIB_LORAWAN_NONCES_BUF_SIZE];
    memcpy(buf, this->getBufferNonces(), sizeof(buf));  // freshly signed copy
    hton<uint16_t>(&buf[RADIOLIB_LORAWAN_NONCES_DEV_NONCE], nonce);
    uint16_t sig = checkSum16(buf, RADIOLIB_LORAWAN_NONCES_BUF_SIZE - 2);
    hton<uint16_t>(&buf[RADIOLIB_LORAWAN_NONCES_SIGNATURE], sig);
    int16_t st = this->setBufferNonces(buf);
    if (st != RADIOLIB_ERR_NONE) {
      printf("[jt] inject: setBufferNonces failed: %d\n", st);
      return false;
    }
    return true;
  }
};

// Result of a join campaign, so callers can verify which DevNonce went out.
struct JoinOutcome {
  bool     joined    = false;
  uint16_t devNonce  = 0;
  uint32_t joinNonce = 0;
};

// One join campaign. If `inject` is non-negative, that DevNonce is (re-)injected
// before *every* attempt — RadioLib increments its counter after each transmitted
// JoinRequest, and the point of injection is that the responder hears exactly the
// chosen value regardless of which retry lands on its channel.
static JoinOutcome do_join(NonceTool& node, long inject = -1) {
  JoinOutcome out;
  printf("[jt] join: starting (up to %d attempts on the random join channel)%s\n",
         JOIN_ATTEMPTS, inject >= 0 ? ", injected DevNonce" : "");
  // Force a fresh JoinRequest. clearSession() drops the session but keeps the
  // nonce buffer, so a plain 'j' keeps the counter climbing as 1.0.4 expects.
  node.clearSession();
  for (int i = 1; i <= JOIN_ATTEMPTS; i++) {
    if (inject >= 0 && !node.injectDevNonce((uint16_t)inject)) {
      return out;  // injection failure is fatal for this campaign
    }
    LoRaWANJoinEvent_t ev;
    int st = node.activateOTAA(&ev);
    bool joined = (st == RADIOLIB_LORAWAN_NEW_SESSION);
    printf("[jt]   attempt %d: state=%d devNonce=%u joinNonce=%lu%s\n", i, st,
           ev.devNonce, (unsigned long)ev.joinNonce, joined ? "  JOINED" : "");
    // Self-check: the wire must carry the injected value. A mismatch means the
    // buffer layout assumption broke (RadioLib upgrade?) — abort loudly rather
    // than let the rehearsal test something other than what it claims.
    if (inject >= 0 && ev.devNonce != (uint16_t)inject) {
      printf("[jt] FATAL: sent devNonce=%u but injected %u — nonce-buffer layout "
             "mismatch, do not trust this build\n",
             ev.devNonce, (unsigned)inject);
      return out;
    }
    if (joined) {
      printf("[jt] RESULT joined  devNonce=%u  joinNonce=%lu\n", ev.devNonce,
             (unsigned long)ev.joinNonce);
      out.joined = true;
      out.devNonce = ev.devNonce;
      out.joinNonce = ev.joinNonce;
      return out;
    }
    sleep_ms(ATTEMPT_GAP_MS);
  }
  printf("[jt] RESULT no-join after %d attempts "
         "(responder silent, or rejecting this DevNonce as a replay)\n",
         JOIN_ATTEMPTS);
  return out;
}

int main() {
  stdio_init_all();
  sleep_ms(2000);  // let USB CDC attach so the first lines are not lost

  PicoHal* hal = new PicoHal(SPI_PORT, PIN_MISO, PIN_MOSI, PIN_SCK);
  SX1262 radio = new Module(hal, PIN_NSS, PIN_DIO1, PIN_RST, PIN_BUSY);
  NonceTool node(&radio, &EU868);

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

  printf("[jt] ready. DevEUI=%016llX  —  'j' join (counter nonce), "
         "'r' join (random nonce), 'p' replay last random nonce, 's' session\n",
         (unsigned long long)JT_DEV_EUI);

  // Last DevNonce sent by 'r', so 'p' can replay it deterministically. -1 = none.
  long last_random_nonce = -1;

  absolute_time_t beat = make_timeout_time_ms(8000);
  while (true) {
    int c = getchar_timeout_us(0);
    if (c == 'j' || c == 'J') {
      do_join(node);
    } else if (c == 'r' || c == 'R') {
      // 1.0.2 emulation: a fresh uniformly-random DevNonce, like LMIC's
      // os_getRndU2(). Non-monotonic by design — a correct (windowed) responder
      // admits every distinct value; the old counter rule rejected ~half.
      uint16_t nonce = (uint16_t)get_rand_32();
      printf("[jt] random join: injected devNonce=%u\n", nonce);
      JoinOutcome out = do_join(node, nonce);
      if (out.joined) last_random_nonce = nonce;
    } else if (c == 'p' || c == 'P') {
      if (last_random_nonce < 0) {
        printf("[jt] replay: no prior 'r' join to replay\n");
      } else {
        // Genuine replay: the exact nonce the responder already accepted. A
        // correct responder rejects every attempt — expect RESULT no-join.
        printf("[jt] replay join: re-sending devNonce=%ld (expect rejection)\n",
               last_random_nonce);
        do_join(node, last_random_nonce);
      }
    } else if (c == 's' || c == 'S') {
      printf("[jt] session active=%d\n", node.isActivated());
    }
    if (time_reached(beat)) {
      printf("[jt] idle · 'j' counter-join · 'r' random-join · 'p' replay\n");
      beat = make_timeout_time_ms(15000);
    }
    sleep_ms(20);
  }
}
