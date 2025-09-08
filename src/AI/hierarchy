M-Bus Protocol Library

├── Data Handling
│   ├── Encoding and Decoding
│   │   ...
│   ├── Data Record (DR)
│   │   ├── Fixed-Length DR
│   │   │   ├── Parsing:  Parses the fixed-length data record structure, extracting fields like device ID (4 bytes), transmission counter (1 byte), status (1 byte), medium type (1 byte), unit (1 byte), and counter values (8 bytes).
│   │   │   ├── Value Extraction: Retrieves the raw counter values from the data record.
│   │   │   └── Unit Lookup: Uses a table to determine the unit of measurement for the counter values based on the `medium_unit` field.
│   │   └── Variable-Length DR
│   │       ├── Data Information Block (DIB)
│   │       │   ├── DIF (Data Information Field):  Provides information about the data type, length, storage number, function (instantaneous, maximum, minimum, etc.), and other data-related flags.
│   │       │   │   └── Data Length Lookup: Determines the data length based on the DIF code, handling both fixed and variable-length data.
│   │       │   ├── DIFE (DIF Extension):  Provides additional information for the data record, such as storage number, tariff, and device ID.
│   │       │   └── Parsing: Parses the DIB, extracting the DIF, DIFE information, and the data length.
│   │       ├── Value Information Block (VIB)
│   │       │   ├── VIF (Value Information Field):  Indicates the unit and quantity of the data.
│   │       │   │   └── Unit Lookup: Determines the unit of measurement based on the VIF code.
│   │       │   ├── VIFE (VIF Extension):  Provides additional information about the unit, including scaling factors and offsets.
│   │       │   └── Parsing: Parses the VIB, extracting the VIF, VIFE, and custom VIF information.
│   │       ├── Data Parsing:  Decodes the data field based on the DIF code.
│   │       ├── Value Extraction:  Retrieves the value from the parsed data, potentially applying normalization based on the VIF and VIFE information.
│   │       └── Unit Normalization:  Converts the raw unit and value to a normalized representation, ensuring consistency and making it easier for applications to process the data.
│   └── Data Normalization
│       ...
├── Address Handling
│   ...
├── Probe Mechanism
│   ...
├── Utility Functions
│   ...
└── M-Bus Initialization
    ...
