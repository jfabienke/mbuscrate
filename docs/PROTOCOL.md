# M-Bus Protocol Reference

## Table of Contents
- [Protocol Overview](#protocol-overview)
- [Frame Formats](#frame-formats)
- [Addressing Modes](#addressing-modes)
- [Control Information](#control-information)
- [Data Structures](#data-structures)
- [Communication Sequences](#communication-sequences)
- [Data Encoding](#data-encoding)
- [Error Handling](#error-handling)

## Protocol Overview

The M-Bus (Meter-Bus) protocol is defined by the European standard EN 13757, designed for remote reading of utility meters. It operates on a master-slave architecture where a central station (master) communicates with multiple meter devices (slaves).

### Key Characteristics
- **Master-Slave Architecture**: Single master, up to 250 slaves
- **Half-Duplex Communication**: Bidirectional, but not simultaneous
- **Collision Avoidance**: Time-slot based communication
- **Error Detection**: Checksum validation on all frames
- **Multi-Telegram Support**: Large data split across frames

## Frame Formats

### Frame Structure Overview

All M-Bus frames follow specific formats with start/stop delimiters and checksums:

```
┌─────────────────────────────────────────────────────────────────┐
│ Start │ Length │ Length │ Start │ C │ A │ CI │ Data │ CS │ Stop │
│ (0x68)│  (L)   │  (L)   │(0x68) │   │   │    │      │    │(0x16)│
└─────────────────────────────────────────────────────────────────┘
```

### 1. ACK Frame (Single Byte)
**Format**: `0xE5`

**Purpose**: Acknowledgment of received frame

**Usage**:
- Slave acknowledges master request
- Master acknowledges slave response

### 2. Short Frame (5 bytes)
**Format**: `[0x10][Control][Address][Checksum][0x16]`

**Structure**:
```
┌──────┬─────────┬─────────┬──────────┬──────┐
│ 0x10 │ Control │ Address │ Checksum │ 0x16 │
│ 1B   │ 1B      │ 1B      │ 1B       │ 1B   │
└──────┴─────────┴─────────┴──────────┴──────┘
```

**Usage**:
- SND_NKE: Initialize slave
- REQ_UD2: Request user data

### 3. Control Frame (9 bytes)
**Format**: `[0x68][0x03][0x03][0x68][Control][Address][CI][Checksum][0x16]`

**Structure**:
```
┌──────┬──────┬──────┬──────┬─────────┬─────────┬────┬──────────┬──────┐
│ 0x68 │ 0x03 │ 0x03 │ 0x68 │ Control │ Address │ CI │ Checksum │ 0x16 │
│ 1B   │ 1B   │ 1B   │ 1B   │ 1B      │ 1B      │ 1B │ 1B       │ 1B   │
└──────┴──────┴──────┴──────┴─────────┴─────────┴────┴──────────┴──────┘
```

**Usage**:
- Extended control commands
- Application reset

### 4. Long Frame (Variable Length)
**Format**: `[0x68][Len][Len][0x68][Control][Address][CI][Data...][Checksum][0x16]`

**Structure**:
```
┌──────┬─────┬─────┬──────┬─────────┬─────────┬────┬──────────┬──────────┬──────┐
│ 0x68 │ Len │ Len │ 0x68 │ Control │ Address │ CI │   Data   │ Checksum │ 0x16 │
│ 1B   │ 1B  │ 1B  │ 1B   │ 1B      │ 1B      │ 1B │ 0-252B   │ 1B       │ 1B   │
└──────┴─────┴─────┴──────┴─────────┴─────────┴────┴──────────┴──────────┴──────┘
```

**Length Field**: `Len = 3 + len(Data)` (Control + Address + CI + Data bytes)

**Maximum Data**: 252 bytes (255 - 3 header bytes)

## Addressing Modes

### 1. Primary Addressing
Direct addressing using 8-bit address field:

| Address   | Description                     |
|-----------|---------------------------------|
| 0x00      | Unconfigured/Factory default    |
| 0x01-0xFA | Valid primary addresses (1-250) |
| 0xFB-0xFD | Reserved                        |
| 0xFE      | Test address                    |
| 0xFF      | Broadcast (no reply expected)   |

### 2. Secondary Addressing
16-digit hexadecimal address for unique identification:

**Format**: `IIIIIIIIMMMMVVVV`
- `IIIIIIII`: 8-digit ID (BCD)
- `MMMM`: 4-digit Manufacturer (encoded)
- `VV`: Version
- `VV`: Medium type

**Selection Process**:
1. Send select command with secondary address
2. Device matches address and activates
3. Use address 0xFD for subsequent communication

### 3. Wildcard Addressing
Use 'F' as wildcard in secondary address:
- `FFFFFFFF12345678`: Match any ID with specific manufacturer
- `12345678FFFFFFFF`: Match specific ID with any manufacturer

## Control Information

### Control Field Bits
```
Bit 7  6  5  4  3  2  1  0
    │  │  │  │  │  └─┴─┴─> Function Code
    │  │  │  │  └────────> FCV (Frame Count Valid)
    │  │  │  └───────────> FCB (Frame Count Bit)
    │  │  └──────────────> PRM (Primary)
    │  └─────────────────> Reserved
    └────────────────────> Direction
```

### Common Control Codes

| Code | Name    | Description               |
|------|---------|---------------------------|
| 0x40 | SND_NKE | Initialize slave          |
| 0x53 | SND_UD  | Send user data            |
| 0x5B | REQ_UD2 | Request user data class 2 |
| 0x5A | REQ_UD1 | Request user data class 1 |
| 0x73 | RSP_UD  | Response with user data   |

### FCB (Frame Count Bit) Toggle
- Alternates between 0 and 1 for successive frames
- Ensures frame sequence integrity
- Reset with SND_NKE command

## Data Structures

### DIB (Data Information Block)

**DIF (Data Information Field)**:
```
Bit 7  6  5  4  3  2  1  0
    │  │  │  │  └─┴─┴─┴─> Data Field (type/length)
    │  │  │  └───────────> Extension bit
    │  │  └──────────────> LSB of storage number
    │  └─────────────────> Tariff
    └────────────────────> Storage number
```

**Data Field Encoding (bits 0-3)**:
| Value | Type         | Length   |
|-------|--------------|----------|
| 0x0   | No data      | 0        |
| 0x1   | 8-bit int    | 1        |
| 0x2   | 16-bit int   | 2        |
| 0x3   | 24-bit int   | 3        |
| 0x4   | 32-bit int   | 4        |
| 0x5   | 32-bit real  | 4        |
| 0x6   | 48-bit int   | 6        |
| 0x7   | 64-bit int   | 8        |
| 0x8   | Selection    | 0        |
| 0x9   | 2-digit BCD  | 1        |
| 0xA   | 4-digit BCD  | 2        |
| 0xB   | 6-digit BCD  | 3        |
| 0xC   | 8-digit BCD  | 4        |
| 0xD   | Variable     | variable |
| 0xE   | 12-digit BCD | 6        |
| 0xF   | Special      | special  |

### VIB (Value Information Block)

**VIF (Value Information Field)**:
```
Bit 7  6  5  4  3  2  1  0
    │  └─┴─┴─┴─┴─┴─┴─> Unit and multiplier
    └────────────────> Extension bit
```

**Primary VIF Ranges**:
| Range     | Unit | Description              |
|-----------|-------|-------------------------|
| 0x00-0x07 | Wh    | Energy (10^n-3)         |
| 0x08-0x0F | J     | Energy (10^n)           |
| 0x10-0x17 | m³    | Volume (10^n-6)         |
| 0x18-0x1F | kg    | Mass (10^n-3)           |
| 0x20-0x27 | -     | On time                 |
| 0x28-0x2F | W     | Power (10^n-3)          |
| 0x30-0x37 | J/h   | Power                   |
| 0x38-0x3F | m³/h  | Volume flow (10^n-6)    |
| 0x40-0x47 | m³/min| Volume flow (10^n-7)    |
| 0x48-0x4F | m³/s  | Volume flow (10^n-9)    |
| 0x50-0x57 | kg/h  | Mass flow (10^n-3)      |
| 0x58-0x5B | °C    | Flow temperature        |
| 0x5C-0x5F | °C    | Return temperature      |
| 0x60-0x63 | K     | Temperature difference  |
| 0x64-0x67 | °C    | External temperature    |
| 0x68-0x6B | bar   | Pressure                |
| 0x6C      | -     | Date (Type G)           |
| 0x6D      | -     | Date/Time (Type F)      |
| 0x6E      | -     | Units for H.C.A.        |
| 0x6F      | -     | Reserved                |
| 0x70-0x77 | -     | Averaging duration      |
| 0x78      | -     | Fabrication No          |
| 0x79      | -     | Enhanced ID             |
| 0x7A      | -     | Bus address             |
| 0x7B-0x7E | -     | VIF in following string |
| 0x7F      | -     | Manufacturer specific   |

### DIFE/VIFE Extensions
When extension bit (0x80) is set, next byte provides additional information:
- Storage number (bits 0-3)
- Tariff (bits 4-5)
- Device unit (bits 6-7)

## Communication Sequences

### 1. Initialization (SND_NKE)
```
Master → Slave: [0x10][0x40][Address][Checksum][0x16]
Slave → Master: [0xE5]
```

### 2. Request Data (REQ_UD2)
```
Master → Slave: [0x10][0x5B/0x7B][Address][Checksum][0x16]
Slave → Master: [Long Frame with Data]
Master → Slave: [0xE5]
```

### 3. Secondary Address Selection
```
Master → Slave: [Long Frame with SELECT and Address]
Slave → Master: [No response - internal activation]
Master → Slave: [0x10][0x5B][0xFD][Checksum][0x16]
Slave → Master: [Long Frame with Data]
```

### 4. Multi-Telegram Sequence
```
Master → Slave: REQ_UD2
Slave → Master: RSP_UD with "more data" flag
Master → Slave: REQ_UD2 (repeated)
Slave → Master: RSP_UD with next block
... (continue until no "more data" flag)
```

## Data Encoding

### BCD (Binary Coded Decimal)
Each byte encodes two decimal digits:
```
0x12 = 12 decimal
0x99 = 99 decimal
```

**Multi-byte BCD** (little-endian):
```
[0x34][0x12] = 1234 decimal
[0x78][0x56][0x34][0x12] = 12345678 decimal
```

### Integer Encoding
Little-endian byte order:
```
16-bit: [0x34][0x12] = 0x1234 = 4660
32-bit: [0x78][0x56][0x34][0x12] = 0x12345678
```

### Float Encoding
IEEE 754 single precision (32-bit):
```
[Sign:1][Exponent:8][Mantissa:23]
```

### Date/Time Encoding

**Type F (CP32) - Date & Time**:
```
Byte 0: IIIIIIMM (I=minute, M=mode)
Byte 1: 000HHHHH (H=hour)
Byte 2: YYYHHHHH (Y=year high, D=day)
Byte 3: MMMMYYYY (M=month, Y=year low)
```

**Type G (CP16) - Date**:
```
Byte 0: YYYDDDDD (Y=year high, D=day)
Byte 1: MMMMYYYY (M=month, Y=year low)
```

**Type I (CP48) - Date & Time with seconds**:
```
Byte 0: 00SSSSSS (S=second)
Byte 1: 00MMMMMM (M=minute)
Byte 2: 000HHHHH (H=hour)
Byte 3: YYYDDDDD (Y=year high, D=day)
Byte 4: MMMMYYYY (M=month, Y=year low)
Byte 5: Reserved
```

### Manufacturer Encoding
3-letter code encoded in 2 bytes:
```
Each letter: A=1, B=2, ..., Z=26
ID = ((L1-64) * 32 * 32) + ((L2-64) * 32) + (L3-64)
Range: 0x0421 (AAA) to 0x6B5A (ZZZ)
```

## Error Handling

### Checksum Calculation
Simple 8-bit arithmetic sum of all data bytes:
```rust
let mut checksum: u8 = 0;
checksum = checksum.wrapping_add(control);
checksum = checksum.wrapping_add(address);
checksum = checksum.wrapping_add(ci);
for byte in data {
    checksum = checksum.wrapping_add(byte);
}
```

### Error Detection
1. **Invalid Start/Stop Bytes**: Frame rejection
2. **Length Mismatch**: Frame rejection
3. **Checksum Error**: Request retransmission
4. **Timeout**: Retry with exponential backoff
5. **No Response**: Mark device as offline

### Error Recovery
1. **Retry Mechanism**: 3 attempts with timeout
2. **Frame Count Bit**: Detect duplicates
3. **Initialization**: SND_NKE to reset state
4. **Fallback**: Try different baud rates

## Protocol Extensions

### Manufacturer Specific
VIF = 0x7F allows manufacturer-defined data formats

### Application Specific
CI field determines application layer:
- 0x51: Data send
- 0x52: Selection
- 0x72: Fixed data structure
- 0x78-0x7F: Manufacturer specific

### Security Features
- Authentication (optional)
- Encryption (AES-128 in wireless)
- Access control levels

## Timing Requirements

### Bit Times
Time for one bit transmission:
- 300 baud: 3.33ms
- 2400 baud: 0.42ms
- 9600 baud: 0.10ms

### Inter-frame Delays
- Minimum: 33 bit times
- Maximum: 330 bit times (timeout)

### Response Times
- Slave response: 11-330 bit times
- Master acknowledgment: 11-50 bit times

## Compliance Notes

This implementation follows:
- **EN 13757-2**: Physical and Link Layer
- **EN 13757-3**: Application Layer
- **EN 13757-4**: Wireless M-Bus (future)
- **OMS**: Open Metering System specifications
