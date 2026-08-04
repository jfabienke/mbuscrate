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

The 602 documentation lists M-Bus, radio, and wireless M-Bus C1 module
variants, but the repositories contain no wireless on-air event examples.
Before adding a wM-Bus event decoder, correlate an optical INFO/KMP read with
captured radio frames and retain the raw vendor records, meter model, and
configuration number.
