# M-Bus Manufacturer Database Reference

## Overview

This document provides a comprehensive reference for M-Bus manufacturer IDs as implemented in the `mbus-rs` crate. The manufacturer ID encoding follows the **EN 13757-3** standard and uses the FLAG Association algorithm.

## Encoding Algorithm

The 16-bit manufacturer ID is encoded from a three-letter code (A–Z) using the formula:

```
ID = (L₁ - 64) × 32² + (L₂ - 64) × 32 + (L₃ - 64)
```

Where L₁, L₂, L₃ are the ASCII codes of the three letters.

### Bit Layout
- **Bits 14-10**: First character (5 bits)
- **Bits 9-5**: Second character (5 bits)
- **Bits 4-0**: Third character (5 bits)
- **Bit 15 (MSB)**: Hard/Soft address flag
  - 0 = Globally unique address (manufacturer guarantee)
  - 1 = Locally unique address (installation-specific)

### Valid Range
- Minimum: `0x0421` (AAA)
- Maximum: `0x6B5A` (ZZZ)

## Manufacturer Categories

### Heat Cost Allocator (HCA) Manufacturers

| ID (Hex) | Code | Manufacturer | Notes |
|----------|------|--------------|-------|
| 0x4493 | QDS | Qundis GmbH | ⚠️ Has proprietary VIF 0x04 date encoding |
| 0x0907 | BHG | Brunata Hürth | German HCA manufacturer |
| 0x2674 | IST | ista International | Major HCA provider |
| 0x5068 | TCH | Techem GmbH | German energy services |
| 0x6A4D | ZRM | Minol Zenner Group | Minol-Zenner partnership |

### Water Meter Manufacturers

| ID (Hex) | Code | Manufacturer | Notes |
|----------|------|--------------|-------|
| 0x05B4 | AMT | Aquametro AG | Swiss manufacturer |
| 0x2324 | HYD | Diehl Metering (Hydrometer) | German water meters |
| 0x68AE | ZEN | Zenner International | Major water meter vendor |
| 0x1596 | ELV | Elvaco | Swedish M-Bus specialists |
| 0x34B4 | MET | Metrix | French manufacturer |

### Heat/Energy Meter Manufacturers

| ID (Hex) | Code | Manufacturer | Notes |
|----------|------|--------------|-------|
| 0x4DEE | SON | Sontex SA | Swiss heat/cold meters |
| 0x4024 | PAD | PadMess GmbH | German measurement tech |
| 0x48AC | REL | Relay GmbH | M-Bus converters |
| 0x14C5 | EFE | Efe | Turkish manufacturer |
| 0x15C7 | ENG | Engelmann | German heat meters |

### Multi-Utility Manufacturers

| ID (Hex) | Code | Manufacturer | Notes |
|----------|------|--------------|-------|
| 0x0442 | ABB | ABB (Asea Brown Boveri) | Global industrial |
| 0x0477 | ACW | Actaris (Itron) | Now part of Itron |
| 0x15A8 | EMH | EMH Energie-Messtechnik | German energy meters |
| 0x15B5 | EMU | EMU Electronic AG | Swiss electronics |
| 0x2697 | ITW | Itron | Global utility solutions |
| 0x2C2D | KAM | Kamstrup | Danish smart meters |
| 0x32A7 | LUG | Landis+Gyr | Swiss/German meters |
| 0x3B52 | NZR | Neue Zählerwerke | German meters |
| 0x4CAE | SEN | Sensus Metering Systems | US/German meters |
| 0x4D25 | SIE | Siemens | German industrial |

### Gas Meter Manufacturers

| ID (Hex) | Code | Manufacturer | Notes |
|----------|------|--------------|-------|
| 0x1593 | ELS | Elster (Honeywell) | Major gas meter vendor |
| 0x4965 | RKE | Raiffeisen Leasing | Austrian provider |

### Other/Specialized Manufacturers

| ID (Hex) | Code | Manufacturer | Notes |
|----------|------|--------------|-------|
| 0x1347 | DZG | DZG Metering | German manufacturer |
| 0x3265 | LSE | LSE Industrie-Elektronik | Industrial electronics |

### Reference/Test

| ID (Hex) | Code | Manufacturer | Notes |
|----------|------|--------------|-------|
| 0x0CAE | CEN | Example Manufacturer | Used in M-Bus documentation |

## Usage Examples

### Encoding a Manufacturer Code

```rust
use mbus_rs::manufacturer_to_id;

// Standard encoding
let id = manufacturer_to_id("KAM").unwrap();
assert_eq!(id, 0x2C2D);

// Case insensitive
let id = manufacturer_to_id("kam").unwrap();
assert_eq!(id, 0x2C2D);
```

### Decoding a Manufacturer ID

```rust
use mbus_rs::id_to_manufacturer;

// Basic decoding
let code = id_to_manufacturer(0x2C2D);
assert_eq!(code, "KAM");

// With MSB set (soft address)
let code = id_to_manufacturer(0xAC2D);
assert_eq!(code, "KAM"); // MSB is automatically masked
```

### Checking for Vendor-Specific Quirks

```rust
use mbus_rs::{has_quirks, get_manufacturer_info};

// Check if manufacturer needs special handling
if has_quirks(0x4493) {  // QDS/Qundis
    println!("This manufacturer has known M-Bus quirks");
}

// Get detailed information
if let Some(info) = get_manufacturer_info(0x4493) {
    println!("Manufacturer: {} ({})", info.name, info.code);
    if let Some(desc) = info.description {
        println!("Notes: {}", desc);
    }
}
```

### MSB (Hard/Soft Address) Handling

```rust
use mbus_rs::{is_soft_address, set_soft_address};

let id = 0x2C2D;  // KAM with hard address

// Check address type
assert!(!is_soft_address(id));

// Convert to soft address
let soft_id = set_soft_address(id, true);
assert_eq!(soft_id, 0xAC2D);
assert!(is_soft_address(soft_id));
```

## Verification Examples

### Example 1: CEN (Reference Implementation)

- Code: C-E-N
- ASCII: C(67), E(69), N(78)
- Values: C(3), E(5), N(14)
- Calculation: 3×1024 + 5×32 + 14 = 3072 + 160 + 14 = **3246 (0x0CAE)**

### Example 2: KAM (Kamstrup)

- Code: K-A-M
- ASCII: K(75), A(65), M(77)
- Values: K(11), A(1), M(13)
- Calculation: 11×1024 + 1×32 + 13 = 11264 + 32 + 13 = **11309 (0x2C2D)**

### Example 3: QDS (Qundis)

- Code: Q-D-S
- ASCII: Q(81), D(68), S(83)
- Values: Q(17), D(4), S(19)
- Calculation: 17×1024 + 4×32 + 19 = 17408 + 128 + 19 = **17555 (0x4493)**

## Common Issues and Solutions

### Issue: Byte Order Confusion

Some documentation may show QDS as `0x5153`. This is incorrect and results from reading the ASCII values directly:
- 'Q' = 0x51, 'S' = 0x53
- The correct encoded value is **0x4493**

### Issue: ELS Encoding Discrepancy

Some sources list ELS as `0x1583`, but the correct calculation gives:
- E(5)×1024 + L(12)×32 + S(19) = **0x1593**

## Standards and References

- **EN 13757-3**: Communication systems for meters - Part 3: M-Bus
- **EN 61107/62056-21**: Electricity metering data exchange
- **DLMS User Association**: Official registry of manufacturer codes
- **libmbus**: Open-source M-Bus protocol implementation

## Database Statistics

- **Total Manufacturers**: 32
- **Categories**: 7 (HCA, Water, Heat/Energy, Multi-utility, Gas, Specialized, Reference)
- **Manufacturers with Quirks**: 1 (Qundis/QDS)
- **Valid ID Range**: 0x0421 - 0x6B5A (1057 - 27482)

## Contributing

To add a new manufacturer:

1. Calculate the ID using the standard formula
2. Add entry to appropriate category in `src/vendors/manufacturer.rs`
3. Add test case in `test_new_manufacturers_encoding()`
4. Update this documentation

## Version History

- **v1.0.0**: Initial database with 19 manufacturers
- **v1.1.0**: Expanded to 32 manufacturers with categorization
- **v1.1.1**: Added MSB handling for hard/soft addresses