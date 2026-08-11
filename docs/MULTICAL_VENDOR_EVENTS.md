# Kamstrup MULTICAL Vendor-Specific Events

This document records vendor-specific event and diagnostic data found in the
Kamstrup MULTICAL references in the sibling `kamstrup-meters` repositories.
It is reference material for future adapter work; it is **not** evidence that
these fields have been decoded from this project's wireless M-Bus captures.

## Current INFO state

The KMP register map identifies these registers:

| Register | Decimal | Meaning |
| --- | ---: | --- |
| `0x0063` | 99 | Current `INFO` code bitmask |
| `0x0071` | 113 | Info-event counter (increments when INFO changes) |
| `0x00AF` | 175 | Error-hour counter (hours with INFO > 0) |

MULTICAL 602 documents the following INFO values. Active values are added
together, so the field is a bitmask rather than an enum:

| Value | Condition |
| ---: | --- |
| 1 | Supply voltage interruption |
| 4, 8, 32 | T2, T1, or T3 temperature sensor out of range |
| 64 | Leak in the cold-water system |
| 256 | Leak in the heating system |
| 512 | Burst in the heating system |
| 16, 128, 1024, 2048 | ULTRAFLOW V1/V2 communication or meter-factor fault |
| 4096, 8192 | ULTRAFLOW V1/V2 signal too low (air) |
| 16384, 32768 | ULTRAFLOW V1/V2 wrong flow direction |

MULTICAL 66C documents the common values `1`, `4`, `8`, `32`, `64`, `256`,
and `512`. Older MULTICAL III meters use a different table (`2` water-meter
fault, `4`/`8` probe faults, `128` battery, `256` excessive pulses, and
`512` system fault). Decode tables must therefore be selected by model and
configuration, not applied globally.

## Event history and logger data

The MULTICAL 602 info logger retains the last 50 INFO changes (36 visible on
the display). A logged event contains the date and INFO code; time and energy
context (E1/E3) are available through the logger tools. The 66C documentation
describes a ten-event info logger.

The MULTICAL 601/801 KMP logger specification defines additional commands:

| CID | Command |
| --- | --- |
| `A0` | Read log from a timestamp toward the present |
| `A1` | Read records after the last read record |
| `A2` | Read log from a record ID toward the present |
| `A3` | Read log from a timestamp toward the past |
| `9B` | Get four event/log unread-status bytes |
| `9C` | Clear event status (conditional clear supported on specified loggers) |

For `9B`, the documented status bits are in status byte 2: bit 0 means the
interval log has unread records and bit 1 means the RTC log has unread records.
These are logger-availability flags, not the current meter fault bitmask.

## Wireless research findings

The public wireless documentation narrows the implementation considerably, but
does not provide complete event-history vectors.

### Confirmed wireless facts

- The MULTICAL 602 C-mode module advertises `Info code` in both its standard and
  alternative current-data packages. These modules are encrypted, one-way C1,
  and transmit every 16 seconds; the fixed-network C-mode module transmits every
  96 seconds. The data-update interval can differ from the RF transmission
  interval, so the gateway must measure both where possible.
  ([602 C-mode datasheet](https://kamstrup-delivery.sitecorecontenthub.cloud/api/public/content/62343-downloadOriginal?v=a39970bb),
  [fixed-network module datasheet](https://kamstrup-delivery.sitecorecontenthub.cloud/api/public/content/61253-downloadOriginal?v=c8cf212d))
- Kamstrup's newer 403/603/803 profile document identifies register `369` as
  `Info bits` in many C1 datagrams. This confirms that current status can be an
  ordinary OMS/M-Bus data record rather than a proprietary CI payload.
  ([Logger Profiles and Datagrams](https://kamstrup-delivery.sitecorecontenthub.cloud/api/public/content/63144-downloadOriginal?v=a62bca35))
- The MULTICAL 602 technical description defines INFO as an additive bitmask,
  documents response/settling times, and says the event counter increments when
  the INFO value changes. Those timings describe meter state changes, not proof
  of an immediate wireless transmission.
  ([602 technical description](https://www.heatingandprocess.com/wp-content/uploads/2016/04/Kamstrup-Multical-602-Heat-Meter-MID-Approved-Technical-Description.pdf))
- A public IM3060 LoRaWAN module specification lists MULTICAL 602 compatibility
  and maps `0x0063` (INFO), `0x0071` (INFOEV), and `0x00AF`
  (ERRORHOURCOUNTER). Its application payload is a header followed by register
  IDs and values. This is useful evidence for an IM3060-like module, but it is
  not evidence that arbitrary raw LoRa packets use the same format.
  ([IM3060 payload specification](https://www.nasys.no/wp-content/uploads/LoRaWAN_Multical_Module_IM3060.pdf))
- Kamstrup's current built-in LoRaWAN product documentation names MULTICAL
  403/603/803, not 602. The meter module must therefore be identified before
  selecting a LoRa decoder.
  ([Kamstrup LoRaWAN announcement](https://www.kamstrup.com/en-en/news-and-events/news/lorawan-communication-module))

### Remaining capture-dependent gaps

1. The exact decrypted 602 wM-Bus DIF/VIF record for INFO (width, encoding,
   ordering, and profile-dependent presence) is not published in the documents
   found here.
2. There is no evidence that the 602 periodic wM-Bus telegram carries the
   50-entry INFO history, INFO-event counter, or error-hour counter. Those may be
   optical/KMP-only logger values.
3. The gateway's LoRa path currently returns raw payload bytes and metadata;
   it does not implement LoRaWAN MAC/session processing. A LoRaWAN module needs
   network/application keys, frame counters, ports, and a post-MAC payload
   decoder. A raw point-to-point LoRa device needs a separate application
   protocol specification.
4. Event tables vary by model, firmware, configuration code, and installed
   module. A single global Kamstrup bit table would be unsafe.
5. The repository has no KAM extension registered yet, and
   [`WMBusHandle`](../src/wmbus/handle.rs) performs structural frame parsing
   while the LoRa branch remains intentionally raw.

### Required capture set

For each supported meter/module combination, retain the model, firmware,
configuration, module part number, raw RF bytes, timestamps, radio mode, RSSI,
decryption key or known plaintext, and corresponding optical reads of `0x0063`,
`0x0071`, and `0x00AF`. Include normal frames and frames correlated with known
INFO changes. Do not infer wireless event-history support from optical KMP
records alone.

## Evidence and implementation status

Primary references are the [MULTICAL 602 technical description](</Users/jvindahl/Development/kamstrup-meters/MeterLogger/documentation/Technical_description_MULTICAL_602.pdf>),
[MULTICAL 66C technical description](</Users/jvindahl/Development/kamstrup-meters/MeterLogger/documentation/MC 66C Technical Description 5511-634 GB Rev C1.pdf>),
[MULTICAL III description](</Users/jvindahl/Development/kamstrup-meters/MeterLogger/documentation/mc-gb.pdf>),
and the [KMP 601/801 specification](</Users/jvindahl/Development/kamstrup-meters/MeterLogger/documentation/5512447_M1_GB.pdf>).
The source maps the INFO and counter registers in
[`kmp.c`](/Users/jvindahl/Development/kamstrup-meters/MeterLogger/user/kamstrup/kmp.c:29)
and [`KMP.m`](/Users/jvindahl/Development/kamstrup-meters/iphonedatalogger/KMP.m:87),
but the MeterLogger sampling loop currently requests only eight measurement
registers ([`kmp_request.c`](/Users/jvindahl/Development/kamstrup-meters/MeterLogger/user/kamstrup/kmp_request.c:354)).
No repository source implements the `A0`–`A3`, `9B`, or `9C` logger commands.

The sibling repositories still contain no wireless on-air event examples.
Until the capture set above exists, implementation should preserve unknown
records and expose the raw INFO value rather than claim complete event-history
support.
