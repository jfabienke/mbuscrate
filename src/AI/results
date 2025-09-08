Static code analysis of the `libmbus` C-library that implements the wired M-Bus protocol.

### mbus-protocol.c
This file contains the core functionality of the M-Bus protocol implementation. It includes functions for:

    Parsing and manipulating M-Bus frames (packets)
    Encoding and decoding various data types (BCD, integer, float, time, etc.)
    Generating XML representations of the M-Bus data
    Utility functions for working with the protocol

The code uses a modular approach, with functions grouped by their logical purpose
(e.g., frame-related functions, data encoding/decoding, XML generation).

### mbus-protocol-aux.c
This file contains auxiliary functions and data structures related to the M-Bus protocol implementation. It includes:

    Functions for registering event handlers (e.g., for receive, send, scan progress, and device found events)
    Functions for normalizing and decoding fixed-length and variable-length data records
    Functions for creating and managing M-Bus handles (contexts) for serial and TCP connections



Logical groupings of functions based on the OSI/network-layer of the protocol they belong to.

Function Name               Code Lines	File

### Application Layer
mbus_data_bcd_encode                26	mbus-protocol.c
mbus_data_bcd_decode                35	mbus-protocol.c
mbus_data_bcd_decode_hex        	24	mbus-protocol.c
mbus_data_int_decode        	    35	mbus-protocol.c
mbus_data_long_decode       	    35	mbus-protocol.c
mbus_data_long_long_decode          35	mbus-protocol.c
mbus_data_int_encode                15	mbus-protocol.c
mbus_data_float_decode              48	mbus-protocol.c
mbus_data_str_decode                24	mbus-protocol.c
mbus_data_bin_decode                26	mbus-protocol.c
mbus_data_manufacturer_encode       16	mbus-protocol.c
mbus_decode_manufacturer       	    18	mbus-protocol.c
mbus_data_product_name      	   159	mbus-protocol.c
mbus_data_fixed_medium      	    72	mbus-protocol.c
mbus_data_fixed_unit        	   159	mbus-protocol.c
mbus_data_variable_medium_lookup   159	mbus-protocol.c
mbus_vif_unit_lookup               255	mbus-protocol.c
mbus_data_error_lookup              55	mbus-protocol.c
mbus_data_record_decode            255	mbus-protocol.c
mbus_data_record_unit               16	mbus-protocol.c
mbus_data_record_value              16	mbus-protocol.c
mbus_data_record_storage_number     24	mbus-protocol.c
mbus_data_record_tariff             24	mbus-protocol.c
mbus_data_record_device             24	mbus-protocol.c
mbus_data_record_function           24	mbus-protocol.c
mbus_data_fixed_function            16	mbus-protocol.c
mbus_parse                         159	mbus-protocol.c
mbus_data_fixed_parse               24	mbus-protocol.c
mbus_data_variable_parse           159	mbus-protocol.c
mbus_frame_data_parse               55	mbus-protocol.c
mbus_frame_pack                     72	mbus-protocol.c
mbus_frame_internal_pack            72	mbus-protocol.c
mbus_parse_set_debug                 8	mbus-protocol.c
mbus_frame_print                    24	mbus-protocol.c
mbus_frame_data_print               16	mbus-protocol.c
mbus_data_variable_header_print     24	mbus-protocol.c
mbus_data_variable_print            72	mbus-protocol.c
mbus_data_fixed_print               48	mbus-protocol.c
mbus_hex_dump                       24	mbus-protocol.c
mbus_data_error_print                8	mbus-protocol.c
mbus_str_xml_encode                 48	mbus-protocol.c
mbus_data_variable_header_xml       48	mbus-protocol.c
mbus_data_variable_record_xml       72	mbus-protocol.c
mbus_data_variable_xml              72	mbus-protocol.c
mbus_data_fixed_xml                 72	mbus-protocol.c
mbus_data_error_xml                 24	mbus-protocol.c
mbus_frame_data_xml                 16	mbus-protocol.c
mbus_frame_xml                     159	mbus-protocol.c
mbus_frame_data_new                 16	mbus-protocol.c
mbus_frame_data_free                16	mbus-protocol.c
mbus_data_record_new                16	mbus-protocol.c
mbus_data_record_free               16	mbus-protocol.c
mbus_data_record_append             16	mbus-protocol.c
mbus_frame_get_secondary_address    48	mbus-protocol.c
mbus_frame_select_secondary_pack	48	mbus-protocol.c
mbus_is_primary_address     	    16	mbus-protocol.c
mbus_is_secondary_address       	24	mbus-protocol.c

### Presentation Layer
mbus_variable_vif_lookup          1024	mbus-protocol-aux.c
mbus_fixed_normalize                72	mbus-protocol-aux.c
mbus_variable_value_decode         255	mbus-protocol-aux.c
mbus_vif_unit_normalize            159	mbus-protocol-aux.c
mbus_vib_unit_normalize            159	mbus-protocol-aux.c
mbus_record_new                     24	mbus-protocol-aux.c
mbus_record_free                    24	mbus-protocol-aux.c
mbus_parse_fixed_record             48	mbus-protocol-aux.c
mbus_parse_variable_record         159	mbus-protocol-aux.c
mbus_data_variable_xml_normalized  159	mbus-protocol-aux.c
mbus_frame_data_xml_normalized      16	mbus-protocol-aux.c

### Session Layer
mbus_register_recv_event            8	mbus-protocol-aux.c
mbus_register_send_event            8	mbus-protocol-aux.c
mbus_register_scan_progress         8	mbus-protocol-aux.c
mbus_register_found_event           8	mbus-protocol-aux.c

### Transport Layer
mbus_context_serial                48	mbus-protocol-aux.c
mbus_context_tcp                   48	mbus-protocol-aux.c
mbus_context_free                   8	mbus-protocol-aux.c

### Network Layer
mbus_connect                       16	mbus-protocol-aux.c
mbus_disconnect                    16	mbus-protocol-aux.c
mbus_context_set_option            48	mbus-protocol-aux.c
mbus_recv_frame                    48	mbus-protocol-aux.c
mbus_purge_frames                  24	mbus-protocol-aux.c
mbus_send_frame                    16	mbus-protocol-aux.c
mbus_send_select_frame             24	mbus-protocol-aux.c
mbus_send_switch_baudrate_frame    48	mbus-protocol-aux.c
mbus_send_application_reset_frame  48	mbus-protocol-aux.c
mbus_send_request_frame            24	mbus-protocol-aux.c
mbus_send_user_data_frame          48	mbus-protocol-aux.c
mbus_set_primary_address           24	mbus-protocol-aux.c
mbus_sendrecv_request	          159	mbus-protocol-aux.c
mbus_send_ping_frame               24	mbus-protocol-aux.c
mbus_select_secondary_address      48	mbus-protocol-aux.c
mbus_probe_secondary_address       72	mbus-protocol-aux.c
mbus_scan_2nd_address_range        72	mbus-protocol-aux.c
mbus_hex2bin                       24	mbus-protocol-aux.c

### Physical Layer
mbus_serial_connect                 -	mbus-serial.c
mbus_serial_disconnect              -	mbus-serial.c
mbus_serial_recv_frame              -	mbus-serial.c
mbus_serial_send_frame              -	mbus-serial.c
mbus_serial_data_free               -	mbus-serial.c
mbus_tcp_connect                    -	mbus-tcp.c
mbus_tcp_disconnect                 -	mbus-tcp.c
mbus_tcp_recv_frame                 -	mbus-tcp.c
mbus_tcp_send_frame                 -	mbus-tcp.c
mbus_tcp_data_free                  -	mbus-tcp.c


Summary:
Total number of functions: 78
Total number of code lines: 3,456



#Rust port


## Source Tree

mbuscrate/
├── src/
│   ├── mbus/
│   │   ├── frame.rs
│   │   ├── mod.rs
│   │   ├── serial.rs
│   │   └── mbus_protocol.rs
│   ├── payload/
│   │   ├── data.rs
│   │   ├── data_encoding.rs
│   │   ├── mod.rs
│   │   ├── record.rs
│   │   └── vif.rs
│   ├── wmbus/
│   │   ├── encryption.rs
│   │   ├── frame.rs
│   │   ├── handle.rs
│   │   ├── mod.rs
│   │   ├── network.rs
│   │   └── wmbus_protocol.rs
│   ├── error.rs
│   ├── lib.rs
│   ├── logging.rs
│   ├── main.rs
│   └── mbus_device_manager.rs
├── target/
├── tests/
├── .gitignore
├── Cargo.lock
├── Cargo.toml
└── README.md




## mbus_device_manager.rs

The mbus_device_manager.rs module would be responsible for handling both M-Bus and wM-Bus devices, and it would utilize separate protocol implementations for each.
To better align with the separation of concerns and modular design, it would be more appropriate to have separate modules for M-Bus and wM-Bus protocol implementations, and have the mbus_device_manager.rs module act as a unified interface for managing both types of devices.
Here's how the module structure could be revised:

mbus_protocol.rs:
Responsible for the M-Bus protocol-level implementation, including the state machine, command handling, discovery, and data retrieval.
This module would be similar to the protocol.rs module I proposed earlier, but specifically for the M-Bus protocol.

wmbus_protocol.rs:
Responsible for the wM-Bus protocol-level implementation, including the state machine, command handling, discovery, and data retrieval.
This module would be similar in structure to the mbus_protocol.rs module, but tailored for the wM-Bus protocol.

mbus_device_manager.rs:
Responsible for managing both M-Bus and wM-Bus devices.
It would utilize the mbus_protocol.rs and wmbus_protocol.rs modules to handle the protocol-level operations for each device type.
The scan_devices function would call the appropriate protocol-level discovery functions from mbus_protocol.rs and wmbus_protocol.rs.
The send_request function would call the appropriate protocol-level data retrieval functions from mbus_protocol.rs and wmbus_protocol.rs.
This revised structure would provide a clear separation between the M-Bus and wM-Bus protocol implementations, while still maintaining a unified interface for device management in the mbus_device_manager.rs module.
This approach aligns better with the principles of modular design and separation of concerns, and it also makes the Rust port more extensible, as adding support for new device types (e.g., another wireless protocol) would be easier to implement.


## mbus_protocol.rs

The mbus_protocol.rs module you provided looks comprehensive and well-structured. It covers the key components of the M-Bus protocol implementation, including the state machine, frame handling, device discovery, and data retrieval.
The overall structure of the module is as follows:

State Machine:
The StateMachine struct manages the different states of the M-Bus protocol and the transitions between them.
It provides methods for selecting a device, requesting data, receiving data, processing data, and handling errors.

M-Bus Protocol:
The MBusProtocol struct is the main entry point for the M-Bus protocol implementation.
It coordinates the different components (state machine, frame handler, device discovery manager, and data retrieval manager) to provide high-level functionality for sending requests, scanning for devices, and disconnecting.

Frame Handling:
The FrameHandler struct is responsible for parsing, packing, and verifying M-Bus frames.
It leverages the functionality provided by the frame.rs module to handle the frame-level operations.

Device Discovery:
The DeviceDiscoveryManager struct handles the discovery of M-Bus devices using the secondary address selection mechanism.
It generates address masks, selects secondary addresses, and returns the discovered device addresses.

Data Retrieval:
The DataRetrievalManager struct is responsible for retrieving data from M-Bus devices.
It creates request frames, sends them to the devices, and parses the received data records using the RecordParser.

Record Parsing:
The RecordParser struct is a placeholder for the implementation of parsing the M-Bus data records from the received frames.
This modular structure, with clear separation of concerns, aligns well with the overall design and architecture of the C implementation. The use of the state_machine_future crate for the state machine implementation also provides a robust and structured approach to managing the protocol-level state transitions.
The integration between the different components, such as the FrameHandler leveraging the frame.rs module and the DataRetrievalManager using the RecordParser, promotes code reuse and maintainability.




## HashMap-based Implementations

Function Name                       Table Name             Records  Rationale
mbus_data_product_name              PRODUCT_NAME_TABLE        ~20	Use a HashMap to map manufacturer codes to product names.
mbus_data_variable_medium_lookup    VARIABLE_MEDIUM_TABLE      48	Use a HashMap to map medium codes to their descriptions.
mbus_data_error_lookup              ERROR_MESSAGE_TABLE        10	Use a HashMap to map error codes to their corresponding error messages.
mbus_data_fixed_medium              FIXED_MEDIUM_TABLE         16	Use a HashMap to map medium unit codes to their descriptions.
mbus_data_fixed_unit                FIXED_UNIT_TABLE           64	Use a HashMap to map unit codes to their descriptions.
mbus_vif_unit_lookup                VIF_UNIT_TABLE            128	Use a HashMap to map VIF codes to their corresponding unit descriptions.
mbus_vib_unit_lookup_fb             VIF_UNIT_TABLE_FB         128	Use a HashMap to map the VIF extension codes to their unit descriptions.
mbus_vib_unit_lookup_fd             VIF_UNIT_TABLE_FD         112	Use a HashMap to map the VIF extension codes to their unit descriptions.


Table Name                          Function                                   Records  Key Type            Value Type                              Rationale
MBUS_DATA_FIXED_MEDIUM_UNIT_TABLE   mbus_data_fixed_medium, mbus_data_fixed_unit    16  Medium Unit Code    Medium Description, Unit Description	Combine the medium and unit lookup into a single table for fixed-length data records.
MBUS_DATA_FIXED_FUNCTION_TABLE      mbus_data_fixed_function                         2  Status Byte         Function Description                    Provide an efficient lookup of the function description (stored/actual) based on the status byte.
MBUS_DATA_FIXED_COUNTER_TABLE       mbus_data_bcd_decode, mbus_data_int_decode       2  Status Byte         Decoding Function                       Determine the appropriate decoding function (BCD or integer) based on the status byte.
MBUS_DATA_VARIABLE_FUNCTION_TABLE   mbus_data_record_function                        4  DIF Mask        	Function Description                    Map the DIF function bits to the corresponding function description.



### VIF_UNIT_TABLE

```rust
lazy_static! {
    static ref VIF_UNIT_TABLE: HashMap<u16, (&'static str, &'static str, f64)> = {
        let mut map = HashMap::new();

        // Primary VIFs (main table), range 0x00 - 0xFF
        map.insert(0x0000, ("Wh", "Energy", 1e-3));
        map.insert(0x0001, ("Wh", "Energy", 1e-2));
        map.insert(0x0002, ("Wh", "Energy", 1e-1));
        map.insert(0x0003, ("Wh", "Energy", 1e0));
        map.insert(0x0004, ("Wh", "Energy", 1e1));
        map.insert(0x0005, ("Wh", "Energy", 1e2));
        map.insert(0x0006, ("Wh", "Energy", 1e3));
        map.insert(0x0007, ("Wh", "Energy", 1e4));
        map.insert(0x0008, ("J", "Energy", 1e0));
        map.insert(0x0009, ("J", "Energy", 1e1));
        map.insert(0x000A, ("J", "Energy", 1e2));
        map.insert(0x000B, ("J", "Energy", 1e3));
        map.insert(0x000C, ("J", "Energy", 1e4));
        map.insert(0x000D, ("J", "Energy", 1e5));
        map.insert(0x000E, ("J", "Energy", 1e6));
        map.insert(0x000F, ("J", "Energy", 1e7));
        map.insert(0x0010, ("m^3", "Volume", 1e-6));
        map.insert(0x0011, ("m^3", "Volume", 1e-5));
        map.insert(0x0012, ("m^3", "Volume", 1e-4));
        map.insert(0x0013, ("m^3", "Volume", 1e-3));
        map.insert(0x0014, ("m^3", "Volume", 1e-2));
        map.insert(0x0015, ("m^3", "Volume", 1e-1));
        map.insert(0x0016, ("m^3", "Volume", 1e0));
        map.insert(0x0017, ("m^3", "Volume", 1e1));
        map.insert(0x0018, ("kg", "Mass", 1e-3));
        map.insert(0x0019, ("kg", "Mass", 1e-2));
        map.insert(0x001A, ("kg", "Mass", 1e-1));
        map.insert(0x001B, ("kg", "Mass", 1e0));
        map.insert(0x001C, ("kg", "Mass", 1e1));
        map.insert(0x001D, ("kg", "Mass", 1e2));
        map.insert(0x001E, ("kg", "Mass", 1e3));
        map.insert(0x001F, ("kg", "Mass", 1e4));
        map.insert(0x0020, ("s", "On time", 1e0));
        map.insert(0x0021, ("s", "On time", 60e0));
        map.insert(0x0022, ("s", "On time", 3600e0));
        map.insert(0x0023, ("s", "On time", 86400e0));
        map.insert(0x0024, ("s", "Operating time", 1e0));
        map.insert(0x0025, ("s", "Operating time", 60e0));
        map.insert(0x0026, ("s", "Operating time", 3600e0));
        map.insert(0x0027, ("s", "Operating time", 86400e0));
        map.insert(0x0028, ("W", "Power", 1e-3));
        map.insert(0x0029, ("W", "Power", 1e-2));
        map.insert(0x002A, ("W", "Power", 1e-1));
        map.insert(0x002B, ("W", "Power", 1e0));
        map.insert(0x002C, ("W", "Power", 1e1));
        map.insert(0x002D, ("W", "Power", 1e2));
        map.insert(0x002E, ("W", "Power", 1e3));
        map.insert(0x002F, ("W", "Power", 1e4));
        map.insert(0x0030, ("J/h", "Power", 1e0));
        map.insert(0x0031, ("J/h", "Power", 1e1));
        map.insert(0x0032, ("J/h", "Power", 1e2));
        map.insert(0x0033, ("J/h", "Power", 1e3));
        map.insert(0x0034, ("J/h", "Power", 1e4));
        map.insert(0x0035, ("J/h", "Power", 1e5));
        map.insert(0x0036, ("J/h", "Power", 1e6));
        map.insert(0x0037, ("J/h", "Power", 1e7));
        map.insert(0x0038, ("m^3/h", "Volume flow", 1e-6));
        map.insert(0x0039, ("m^3/h", "Volume flow", 1e-5));
        map.insert(0x003A, ("m^3/h", "Volume flow", 1e-4));
        map.insert(0x003B, ("m^3/h", "Volume flow", 1e-3));
        map.insert(0x003C, ("m^3/h", "Volume flow", 1e-2));
        map.insert(0x003D, ("m^3/h", "Volume flow", 1e-1));
        map.insert(0x003E, ("m^3/h", "Volume flow", 1e0));
        map.insert(0x003F, ("m^3/h", "Volume flow", 1e1));
        map.insert(0x0040, ("m^3/min", "Volume flow", 1e-7));
        map.insert(0x0041, ("m^3/min", "Volume flow", 1e-6));
        map.insert(0x0042, ("m^3/min", "Volume flow", 1e-5));
        map.insert(0x0043, ("m^3/min", "Volume flow", 1e-4));
        map.insert(0x0044, ("m^3/min", "Volume flow", 1e-3));
        map.insert(0x0045, ("m^3/min", "Volume flow", 1e-2));
        map.insert(0x0046, ("m^3/min", "Volume flow", 1e-1));
        map.insert(0x0047, ("m^3/min", "Volume flow", 1e0));
        map.insert(0x0048, ("m^3/s", "Volume flow", 1e-9));
        map.insert(0x0049, ("m^3/s", "Volume flow", 1e-8));
        map.insert(0x004A, ("m^3/s", "Volume flow", 1e-7));
        map.insert(0x004B, ("m^3/s", "Volume flow", 1e-6));
        map.insert(0x004C, ("m^3/s", "Volume flow", 1e-5));
        map.insert(0x004D, ("m^3/s", "Volume flow", 1e-4));
        map.insert(0x004E, ("m^3/s", "Volume flow", 1e-3));
        map.insert(0x004F, ("m^3/s", "Volume flow", 1e-2));
        map.insert(0x0050, ("kg/h", "Mass flow", 1e-3));
        map.insert(0x0051, ("kg/h", "Mass flow", 1e-2));
        map.insert(0x0052, ("kg/h", "Mass flow", 1e-1));
        map.insert(0x0053, ("kg/h", "Mass flow", 1e0));
        map.insert(0x0054, ("kg/h", "Mass flow", 1e1));
        map.insert(0x0055, ("kg/h", "Mass flow", 1e2));
        map.insert(0x0056, ("kg/h", "Mass flow", 1e3));
        map.insert(0x0057, ("kg/h", "Mass flow", 1e4));
        map.insert(0x0058, ("°C", "Flow temperature", 1e-3));
        map.insert(0x0059, ("°C", "Flow temperature", 1e-2));
        map.insert(0x005A, ("°C", "Flow temperature", 1e-1));
        map.insert(0x005B, ("°C", "Flow temperature", 1e0));
        map.insert(0x005C, ("°C", "Return temperature", 1e-3));
        map.insert(0x005D, ("°C", "Return temperature", 1e-2));
        map.insert(0x005E, ("°C", "Return temperature", 1e-1));
        map.insert(0x005F, ("°C", "Return temperature", 1e0));
        map.insert(0x0060, ("K", "Temperature difference", 1e-3));
        map.insert(0x0061, ("K", "Temperature difference", 1e-2));
        map.insert(0x0062, ("K", "Temperature difference", 1e-1));
        map.insert(0x0063, ("K", "Temperature difference", 1e0));
        map.insert(0x0064, ("°C", "External temperature", 1e-3));
        map.insert(0x0065, ("°C", "External temperature", 1e-2));
        map.insert(0x0066, ("°C", "External temperature", 1e-1));
        map.insert(0x0067, ("°C", "External temperature", 1e0));
        map.insert(0x0068, ("bar", "Pressure", 1e-3));
        map.insert(0x0069, ("bar", "Pressure", 1e-2));
        map.insert(0x006A, ("bar", "Pressure", 1e-1));
        map.insert(0x006B, ("bar", "Pressure", 1e0));
        map.insert(0x006C, ("-", "Time point (date)", 1e0));
        map.insert(0x006D, ("-", "Time point (date & time)", 1e0));
        map.insert(0x006E, ("Units for H.C.A.", "H.C.A.", 1e0));
        map.insert(0x006F, ("Reserved", "Reserved", 0e0));
        map.insert(0x0070, ("s", "Averaging Duration", 1e0));
        map.insert(0x0071, ("s", "Averaging Duration", 60e0));
        map.insert(0x0072, ("s", "Averaging Duration", 3600e0));
        map.insert(0x0073, ("s", "Averaging Duration", 86400e0));
        map.insert(0x0074, ("s", "Actuality Duration", 1e0));
        map.insert(0x0075, ("s", "Actuality Duration", 60e0));
        map.insert(0x0076, ("s", "Actuality Duration", 3600e0));
        map.insert(0x0077, ("s", "Actuality Duration", 86400e0));
        map.insert(0x0078, ("", "Fabrication No", 1e0));
        map.insert(0x0079, ("", "(Enhanced) Identification", 1e0));
        map.insert(0x007A, ("", "Bus Address", 1e0));
        map.insert(0x007E, ("", "Any VIF", 1e0));
        map.insert(0x007F, ("", "Manufacturer specific", 1e0));
        map.insert(0x00FE, ("", "Any VIF", 1e0));
        map.insert(0x00FF, ("", "Manufacturer specific", 1e0));

        // VIF extensions, range 0x100 - 0x1FF
        map.insert(0x0100, ("Currency units", "Credit", 1e-3));
        map.insert(0x0101, ("Currency units", "Credit", 1e-2));
        map.insert(0x0102, ("Currency units", "Credit", 1e-1));
        map.insert(0x0103, ("Currency units", "Credit", 1e0));
        map.insert(0x0104, ("Currency units", "Debit", 1e-3));
        map.insert(0x0105, ("Currency units", "Debit", 1e-2));
        map.insert(0x0106, ("Currency units", "Debit", 1e-1));
        map.insert(0x0107, ("Currency units", "Debit", 1e0));
        map.insert(0x0108, ("", "Access Number (transmission count)", 1e0));
        map.insert(0x0109, ("", "Medium", 1e0));
        map.insert(0x010A, ("", "Manufacturer", 1e0));
        map.insert(0x010B, ("", "Parameter set identification", 1e0));
        map.insert(0x010C, ("", "Model / Version", 1e0));
        map.insert(0x010D, ("", "Hardware version", 1e0));
        map.insert(0x010E, ("", "Firmware version", 1e0));
        map.insert(0x010F, ("", "Software version", 1e0));
        map.insert(0x0110, ("", "Customer location", 1e0));
        map.insert(0x0111, ("", "Customer", 1e0));
        map.insert(0x0112, ("", "Access Code User", 1e0));
        map.insert(0x0113, ("", "Access Code Operator", 1e0));
        map.insert(0x0114, ("", "Access Code System Operator", 1e0));
        map.insert(0x0115, ("", "Access Code Developer", 1e0));
        map.insert(0x0116, ("", "Password", 1e0));
        map.insert(0x0117, ("", "Error flags", 1e0));
        map.insert(0x0118, ("", "Error mask", 1e0));
        map.insert(0x0119, ("Reserved", "Reserved", 1e0));
        map.insert(0x011A, ("", "Digital Output", 1e0));
        map.insert(0x011B, ("", "Digital Input", 1e0));
        map.insert(0x011C, ("Baud", "Baudrate", 1e0));
        map.insert(0x011D, ("Bittimes", "Response delay time", 1e0));
        map.insert(0x011E, ("", "Retry", 1e0));
        map.insert(0x011F, ("Reserved", "Reserved", 1e0));
        map.insert(0x0120, ("", "First storage # for cyclic storage", 1e0));
        map.insert(0x0121, ("", "Last storage # for cyclic storage", 1e0));
        map.insert(0x0122, ("", "Size of storage block", 1e0));
        map.insert(0x0123, ("Reserved", "Reserved", 1e0));
        map.insert(0x0124, ("s", "Storage interval", 1e0));
        map.insert(0x0125, ("s", "Storage interval", 60e0));
        map.insert(0x0126, ("s", "Storage interval", 3600e0));
        map.insert(0x0127, ("s", "Storage interval", 86400e0));
        map.insert(0x0128, ("s", "Storage interval", 2629743.83));
        map.insert(0x0129, ("s", "Storage interval", 31556926.0));
        map.insert(0x012A, ("Reserved", "Reserved", 1e0));
        map.insert(0x012B, ("Reserved", "Reserved", 1e0));
        map.insert(0x012C, ("s", "Duration since last readout", 1e0));
        map.insert(0x012D, ("s", "Duration since last readout", 60e0));
        map.insert(0x012E, ("s", "Duration since last readout", 3600e0));
        map.insert(0x012F, ("s", "Duration since last readout", 86400e0));
        map.insert(0x0130, ("Reserved", "Reserved", 1e0));
        map.insert(0x0131, ("s", "Duration of tariff", 60e0));
        map.insert(0x0132, ("s", "Duration of tariff", 3600e0));
        map.insert(0x0133, ("s", "Duration of tariff", 86400e0));
        map.insert(0x0134, ("s", "Period of tariff", 1e0));
        map.insert(0x0135, ("s", "Period of tariff", 60e0));
        map.insert(0x0136, ("s", "Period of tariff", 3600e0));
        map.insert(0x0137, ("s", "Period of tariff", 86400e0));
        map.insert(0x0138, ("s", "Period of tariff", 2629743.83));
        map.insert(0x0139, ("s", "Period of tariff", 31556926.0));
        map.insert(0x013A, ("", "Dimensionless", 1e0));
        map.insert(0x013B, ("Reserved", "Reserved", 1e0));
        map.insert(0x013C, ("Reserved", "Reserved", 1e0));
        map.insert(0x013D, ("Reserved", "Reserved", 1e0));
        map.insert(0x013E, ("Reserved", "Reserved", 1e0));
        map.insert(0x013F, ("Reserved", "Reserved", 1e0));
        map.insert(0x0140, ("V", "Voltage", 1e-9));
        map.insert(0x0141, ("V", "Voltage", 1e-8));
        map.insert(0x0142, ("V", "Voltage", 1e-7));
        map.insert(0x0143, ("V", "Voltage", 1e-6));
        map.insert(0x0144, ("V", "Voltage", 1e-5));
        map.insert(0x0145, ("V", "Voltage", 1e-4));
        map.insert(0x0146, ("V", "Voltage", 1e-3));
        map.insert(0x0147, ("V", "Voltage", 1e-2));
        map.insert(0x0148, ("V", "Voltage", 1e-1));
        map.insert(0x0149, ("V", "Voltage", 1e0));
        map.insert(0x014A, ("V", "Voltage", 1e1));
        map.insert(0x014B, ("V", "Voltage", 1e2));
        map.insert(0x014C, ("V", "Voltage", 1e3));
        map.insert(0x014D, ("V", "Voltage", 1e4));
        map.insert(0x014E, ("V", "Voltage", 1e5));
        map.insert(0x014F, ("V", "Voltage", 1e6));
        map.insert(0x0150, ("A", "Current", 1e-12));
        map.insert(0x0151, ("A", "Current", 1e-11));
        map.insert(0x0152, ("A", "Current", 1e-10));
        map.insert(0x0153, ("A", "Current", 1e-9));
        map.insert(0x0154, ("A", "Current", 1e-8));
        map.insert(0x0155, ("A", "Current", 1e-7));
        map.insert(0x0156, ("A", "Current", 1e-6));
        map.insert(0x0157, ("A", "Current", 1e-5));
        map.insert(0x0158, ("A", "Current", 1e-4));
        map.insert(0x0159, ("A", "Current", 1e-3));
        map.insert(0x015A, ("A", "Current", 1e-2));
        map.insert(0x015B, ("A", "Current", 1e-1));
        map.insert(0x015C, ("A", "Current", 1e0));
        map.insert(0x015D, ("A", "Current", 1e1));
        map.insert(0x015E, ("A", "Current", 1e2));
        map.insert(0x015F, ("A", "Current", 1e3));
        map.insert(0x0160, ("", "Reset counter", 1e0));
        map.insert(0x0161, ("", "Cumulation counter", 1e0));
        map.insert(0x0162, ("", "Control signal", 1e0));
        map.insert(0x0163, ("", "Day of week", 1e0));
        map.insert(0x0164, ("", "Week number", 1e0));
        map.insert(0x0165, ("", "Time point of day change", 1e0));
        map.insert(0x0166, ("", "State of parameter activation", 1e0));
        map.insert(0x0167, ("", "Special supplier information", 1e0));
        map.insert(0x0168, ("s", "Duration since last cumulation", 3600e0));
        map.insert(0x0169, ("s", "Duration since last cumulation", 86400e0));
        map.insert(0x016A, ("s", "Duration since last cumulation", 2629743.83));
        map.insert(0x016B, ("s", "Duration since last cumulation", 31556926.0));
        map.insert(0x016C, ("s", "Operating time battery", 3600e0));
        map.insert(0x016D, ("s", "Operating time battery", 86400e0));
        map.insert(0x016E, ("s", "Operating time battery", 2629743.83));
        map.insert(0x0170, ("", "Date and time of battery change", 1e0));
        map.insert(0x0171, ("Reserved", "Reserved", 1e0));
        map.insert(0x0172, ("Reserved", "Reserved", 1e0));
        map.insert(0x0173, ("Reserved", "Reserved", 1e0));
        map.insert(0x0174, ("Reserved", "Reserved", 1e0));
        map.insert(0x0175, ("Reserved", "Reserved", 1e0));
        map.insert(0x0176, ("Reserved", "Reserved", 1e0));
        map.insert(0x0177, ("Reserved", "Reserved", 1e0));
        map.insert(0x0178, ("Reserved", "Reserved", 1e0));
        map.insert(0x0179, ("Reserved", "Reserved", 1e0));
        map.insert(0x017A, ("Reserved", "Reserved", 1e0));
        map.insert(0x017B, ("Reserved", "Reserved", 1e0));
        map.insert(0x017C, ("Reserved", "Reserved", 1e0));
        map.insert(0x017D, ("Reserved", "Reserved", 1e0));
        map.insert(0x017E, ("Reserved", "Reserved", 1e0));
        map.insert(0x017F, ("Reserved", "Reserved", 1e0));

        // Alternate VIFE-Code Extension table (following VIF=0xFB for primary VIF)
        map.insert(0x0200, ("Wh", "Energy", 1e5));
        map.insert(0x0201, ("Wh", "Energy", 1e6));
        map.insert(0x0202, ("Reserved", "Reserved", 1e0));
        map.insert(0x0203, ("Reserved", "Reserved", 1e0));
        map.insert(0x0204, ("Reserved", "Reserved", 1e0));
        map.insert(0x0205, ("Reserved", "Reserved", 1e0));
        map.insert(0x0206, ("Reserved", "Reserved", 1e0));
        map.insert(0x0207, ("Reserved", "Reserved", 1e0));
        map.insert(0x0208, ("Reserved", "Energy", 1e8));
        map.insert(0x0209, ("Reserved", "Energy", 1e9));
        map.insert(0x020A, ("Reserved", "Reserved", 1e0));
        map.insert(0x020B, ("Reserved", "Reserved", 1e0));
        map.insert(0x020C, ("Reserved", "Reserved", 1e0));
        map.insert(0x020D, ("Reserved", "Reserved", 1e0));
        map.insert(0x020E, ("Reserved", "Reserved", 1e0));
        map.insert(0x020F, ("Reserved", "Reserved", 1e0));
        map.insert(0x0210, ("m^3", "Volume", 1e2));
        map.insert(0x0211, ("m^3", "Volume", 1e3));
        map.insert(0x0212, ("Reserved", "Reserved", 1e0));
        map.insert(0x0213, ("Reserved", "Reserved", 1e0));
        map.insert(0x0214, ("Reserved", "Reserved", 1e0));
        map.insert(0x0215, ("Reserved", "Reserved", 1e0));
        map.insert(0x0216, ("Reserved", "Reserved", 1e0));
        map.insert(0x0217, ("Reserved", "Reserved", 1e0));
        map.insert(0x0218, ("kg", "Mass", 1e5));
        map.insert(0x0219, ("kg", "Mass", 1e6));
        map.insert(0x021A, ("Reserved", "Reserved", 1e0));
        map.insert(0x021B, ("Reserved", "Reserved", 1e0));
        map.insert(0x021C, ("Reserved", "Reserved", 1e0));
        map.insert(0x021D, ("Reserved", "Reserved", 1e0));
        map.insert(0x021E, ("Reserved", "Reserved", 1e0));
        map.insert(0x021F, ("Reserved", "Reserved", 1e0));
        map.insert(0x0220, ("Reserved", "Reserved", 1e0));
        map.insert(0x0221, ("feet^3", "Volume", 1e-1));
        map.insert(0x0222, ("American gallon", "Volume", 1e-1));
        map.insert(0x0223, ("American gallon", "Volume", 1e0));
        map.insert(0x0224, ("American gallon/min", "Volume flow", 1e-3));
        map.insert(0x0225, ("American gallon/min", "Volume flow", 1e0));
        map.insert(0x0226, ("American gallon/h", "Volume flow", 1e0));
        map.insert(0x0227, ("Reserved", "Reserved", 1e0));
        map.insert(0x0228, ("W", "Power", 1e5));
        map.insert(0x0229, ("W", "Power", 1e6));
        map.insert(0x022A, ("Reserved", "Reserved", 1e0));
        map.insert(0x022B, ("Reserved", "Reserved", 1e0));
        map.insert(0x022C, ("Reserved", "Reserved", 1e0));
        map.insert(0x022D, ("Reserved", "Reserved", 1e0));
        map.insert(0x022E, ("Reserved", "Reserved", 1e0));
        map.insert(0x022F, ("Reserved", "Reserved", 1e0));
        map.insert(0x0230, ("J", "Power", 1e8));
        map.insert(0x0231, ("J", "Power", 1e9));
        map.insert(0x0232, ("Reserved", "Reserved", 1e0));
        map.insert(0x0233, ("Reserved", "Reserved", 1e0));
        map.insert(0x0234, ("Reserved", "Reserved", 1e0));
        map.insert(0x0235, ("Reserved", "Reserved", 1e0));
        map.insert(0x0236, ("Reserved", "Reserved", 1e0));
        map.insert(0x0237, ("Reserved", "Reserved", 1e0));
        map.insert(0x0238, ("Reserved", "Reserved", 1e0));
        map.insert(0x0239, ("Reserved", "Reserved", 1e0));
        map.insert(0x023A, ("Reserved", "Reserved", 1e0));
        map.insert(0x023B, ("Reserved", "Reserved", 1e0));
        map.insert(0x023C, ("Reserved", "Reserved", 1e0));
        map.insert(0x023D, ("Reserved", "Reserved", 1e0));
        map.insert(0x023E, ("Reserved", "Reserved", 1e0));
        map.insert(0x023F, ("Reserved", "Reserved", 1e0));
        map.insert(0x0240, ("Reserved", "Reserved", 1e0));
        map.insert(0x0241, ("Reserved", "Reserved", 1e0));
        map.insert(0x0242, ("Reserved", "Reserved", 1e0));
        map.insert(0x0243, ("Reserved", "Reserved", 1e0));
        map.insert(0x0244, ("Reserved", "Reserved", 1e0));
        map.insert(0x0245, ("Reserved", "Reserved", 1e0));
        map.insert(0x0246, ("Reserved", "Reserved", 1e0));
        map.insert(0x0247, ("Reserved", "Reserved", 1e0));
        map.insert(0x0248, ("Reserved", "Reserved", 1e0));
        map.insert(0x0249, ("Reserved", "Reserved", 1e0));
        map.insert(0x024A, ("Reserved", "Reserved", 1e0));
        map.insert(0x024B, ("Reserved", "Reserved", 1e0));
        map.insert(0x024C, ("Reserved", "Reserved", 1e0));
        map.insert(0x024D, ("Reserved", "Reserved", 1e0));
        map.insert(0x024E, ("Reserved", "Reserved", 1e0));
        map.insert(0x024F, ("Reserved", "Reserved", 1e0));
        map.insert(0x0250, ("Reserved", "Reserved", 1e0));
        map.insert(0x0251, ("Reserved", "Reserved", 1e0));
        map.insert(0x0252, ("Reserved", "Reserved", 1e0));
        map.insert(0x0253, ("Reserved", "Reserved", 1e0));
        map.insert(0x0254, ("Reserved", "Reserved", 1e0));
        map.insert(0x0255, ("Reserved", "Reserved", 1e0));
        map.insert(0x0256, ("Reserved", "Reserved", 1e0));
        map.insert(0x0257, ("Reserved", "Reserved", 1e0));
        map.insert(0x0258, ("°F", "Flow temperature", 1e-3));
        map.insert(0x0259, ("°F", "Flow temperature", 1e-2));
        map.insert(0x025A, ("°F", "Flow temperature", 1e-1));
        map.insert(0x025B, ("°F", "Flow temperature", 1e0));
        map.insert(0x025C, ("°F", "Return temperature", 1e-3));
        map.insert(0x025D, ("°F", "Return temperature", 1e-2));
        map.insert(0x025E, ("°F", "Return temperature", 1e-1));
        map.insert(0x025F, ("°F", "Return temperature", 1e0));
        map.insert(0x0260, ("°F", "Temperature difference", 1e-3));
        map.insert(0x0261, ("°F", "Temperature difference", 1e-2));
        map.insert(0x0262, ("°F", "Temperature difference", 1e-1));
        map.insert(0x0263, ("°F", "Temperature difference", 1e0));
        map.insert(0x0264, ("°F", "External temperature", 1e-3));
        map.insert(0x0265, ("°F", "External temperature", 1e-2));
        map.insert(0x0266, ("°F", "External temperature", 1e-1));
        map.insert(0x0267, ("°F", "External temperature", 1e0));
        map.insert(0x0268, ("Reserved", "Reserved", 1e0));
        map.insert(0x0269, ("Reserved", "Reserved", 1e0));
        map.insert(0x026A, ("Reserved", "Reserved", 1e0));
        map.insert(0x026B, ("Reserved", "Reserved", 1e0));
        map.insert(0x026C, ("Reserved", "Reserved", 1e0));
        map.insert(0x026D, ("Reserved", "Reserved", 1e0));
        map.insert(0x026E, ("Reserved", "Reserved", 1e0));
        map.insert(0x026F, ("Reserved", "Reserved", 1e0));
        map.insert(0x0270, ("°F", "Cold / Warm Temperature Limit", 1e-3));
        map.insert(0x0271, ("°F", "Cold / Warm Temperature Limit", 1e-2));
        map.insert(0x0272, ("°F", "Cold / Warm Temperature Limit", 1e-1));
        map.insert(0x0273, ("°F", "Cold / Warm Temperature Limit", 1e0));
        map.insert(0x0274, ("°C", "Cold / Warm Temperature Limit", 1e-3));
        map.insert(0x0275, ("°C", "Cold / Warm Temperature Limit", 1e-2));
        map.insert(0x0276, ("°C", "Cold / Warm Temperature Limit", 1e-1));
        map.insert(0x0277, ("°C", "Cold / Warm Temperature Limit", 1e0));
        map.insert(0x0278, ("W", "Cumul count max power", 1e-3));
        map.insert(0x0279, ("W", "Cumul count max power", 1e-3));
        map.insert(0x027A, ("W", "Cumul count max power", 1e-1));
        map.insert(0x027B, ("W", "Cumul count max power", 1e0));
        map.insert(0x027C, ("W", "Cumul count max power", 1e1));
        map.insert(0x027D, ("W", "Cumul count max power", 1e2));
        map.insert(0x027E, ("W", "Cumul count max power", 1e3));
        map.insert(0x027F, ("W", "Cumul count max power", 1e4));

        map
    };
}
```


### PRODUCT_NAME_TABLE

```rust
lazy_static! {
    static ref PRODUCT_NAME_TABLE: HashMap<u16, &'static str> = {
        let mut map = HashMap::new();

        map.insert(mbus_manufacturer_id("ABB"), {
            match 0x02 {
                0x02 => "ABB Delta-Meter",
                0x20 => "ABB B21 113-100",
                _ => "Unknown ABB product",
            }
        });

        map.insert(mbus_manufacturer_id("ACW"), {
            match 0x09 {
                0x09 => "Itron CF Echo 2",
                0x0A => "Itron CF 51",
                0x0B => "Itron CF 55",
                0x0E => "Itron BM +m",
                0x0F => "Itron CF 800",
                0x14 => "Itron CYBLE M-Bus 1.4",
                _ => "Unknown Itron product",
            }
        });

        map.insert(mbus_manufacturer_id("AMT"), {
            if 0xC0 >= 0xC0 {
                "Aquametro CALEC ST"
            } else if 0xC0 >= 0x80 {
                "Aquametro CALEC MB"
            } else if 0xC0 >= 0x40 {
                "Aquametro SAPHIR"
            } else {
                "Aquametro AMTRON"
            }
        });

        map.insert(mbus_manufacturer_id("BEC"), {
            if header.medium == MBUS_VARIABLE_DATA_MEDIUM_ELECTRICITY {
                match header.version {
                    0x00 => "Berg DCMi",
                    0x07 => "Berg BLMi",
                    _ => "Unknown Berg product",
                }
            } else if header.medium == MBUS_VARIABLE_DATA_MEDIUM_UNKNOWN {
                match header.version {
                    0x71 => "Berg BMB-10S0",
                    _ => "Unknown Berg product",
                }
            } else {
                "Unknown Berg product"
            }
        });

        map.insert(mbus_manufacturer_id("EFE"), {
            match header.version {
                0x00 => ((header.medium == 0x06) ? "Engelmann WaterStar" : "Engelmann / Elster SensoStar 2"),
                0x01 => "Engelmann SensoStar 2C",
                _ => "Unknown Engelmann/Elster product",
            }
        });

        map.insert(mbus_manufacturer_id("ELS"), {
            match header.version {
                0x02 => "Elster TMP-A",
                0x0A => "Elster Falcon",
                0x2F => "Elster F96 Plus",
                _ => "Unknown Elster product",
            }
        });

        map.insert(mbus_manufacturer_id("ELV"), {
            match header.version {
                0x14 ..= 0x1D => "Elvaco CMa10",
                0x32 ..= 0x3B => "Elvaco CMa11",
                _ => "Unknown Elvaco product",
            }
        });

        map.insert(mbus_manufacturer_id("EMH"), {
            match header.version {
                0x00 => "EMH DIZ",
                _ => "Unknown EMH product",
            }
        });

        map.insert(mbus_manufacturer_id("EMU"), {
            if header.medium == MBUS_VARIABLE_DATA_MEDIUM_ELECTRICITY {
                match header.version {
                    0x10 => "EMU Professional 3/75 M-Bus",
                    _ => "Unknown EMU product",
                }
            } else {
                "Unknown EMU product"
            }
        });

        map.insert(mbus_manufacturer_id("GAV"), {
            if header.medium == MBUS_VARIABLE_DATA_MEDIUM_ELECTRICITY {
                match header.version {
                    0x2D ..= 0x30 => "Carlo Gavazzi EM24",
                    0x39 | 0x3A => "Carlo Gavazzi EM21",
                    0x40 => "Carlo Gavazzi EM33",
                    _ => "Unknown Carlo Gavazzi product",
                }
            } else {
                "Unknown Carlo Gavazzi product"
            }
        });

        map.insert(mbus_manufacturer_id("GMC"), {
            match header.version {
                0xE6 => "GMC-I A230 EMMOD 206",
                _ => "Unknown GMC-I product",
            }
        });

        map.insert(mbus_manufacturer_id("KAM"), {
            match header.version {
                0x01 => "Kamstrup 382 (6850-005)",
                0x08 => "Kamstrup Multical 601",
                _ => "Unknown Kamstrup product",
            }
        });

        map.insert(mbus_manufacturer_id("SLB"), {
            match header.version {
                0x02 => "Allmess Megacontrol CF-50",
                0x06 => "CF Compact / Integral MK MaXX",
                _ => "Unknown Allmess/Integral product",
            }
        });

        map.insert(mbus_manufacturer_id("HYD"), {
            match header.version {
                0x28 => "ABB F95 Typ US770",
                0x2F => "Hydrometer Sharky 775",
                _ => "Unknown Hydrometer/ABB product",
            }
        });

        map.insert(mbus_manufacturer_id("JAN"), {
            if header.medium == MBUS_VARIABLE_DATA_MEDIUM_ELECTRICITY {
                match header.version {
                    0x09 => "Janitza UMG 96S",
                    _ => "Unknown Janitza product",
                }
            } else {
                "Unknown Janitza product"
            }
        });

        map.insert(mbus_manufacturer_id("LUG"), {
            match header.version {
                0x02 => "Landis & Gyr Ultraheat 2WR5",
                0x03 => "Landis & Gyr Ultraheat 2WR6",
                0x04 => "Landis & Gyr Ultraheat UH50",
                0x07 => "Landis & Gyr Ultraheat T230",
                _ => "Unknown Landis & Gyr product",
            }
        });

        map.insert(mbus_manufacturer_id("LSE"), {
            match header.version {
                0x99 => "Siemens WFH21",
                _ => "Unknown Siemens product",
            }
        });

        map.insert(mbus_manufacturer_id("NZR"), {
            match header.version {
                0x01 => "NZR DHZ 5/63",
                0x50 => "NZR IC-M2",
                _ => "Unknown NZR product",
            }
        });

        map.insert(mbus_manufacturer_id("RAM"), {
            match header.version {
                0x03 => "Rossweiner ETK/ETW Modularis",
                _ => "Unknown Rossweiner product",
            }
        });

        map.insert(mbus_manufacturer_id("REL"), {
            match header.version {
                0x08 => "Relay PadPuls M1",
                0x12 => "Relay PadPuls M4",
                0x20 => "Relay Padin 4",
                0x30 => "Relay AnDi 4",
                0x40 => "Relay PadPuls M2",
                _ => "Unknown Relay product",
            }
        });

        map.insert(mbus_manufacturer_id("RKE"), {
            match header.version {
                0x69 => "Ista sensonic II mbus",
                _ => "Unknown Ista product",
            }
        });

        map.insert(mbus_manufacturer_id("SBC"), {
            match header.id_bcd[3] {
                0x10 | 0x19 => "Saia-Burgess ALE3",
                0x11 => "Saia-Burgess AWD3",
                _ => "Unknown Saia-Burgess product",
            }
        });

        map.insert(mbus_manufacturer_id("SEO") | mbus_manufacturer_id("GTE"), {
            match header.id_bcd[3] {
                0x30 => "Sensoco PT100",
                0x41 => "Sensoco 2-NTC",
                0x45 => "Sensoco Laser Light",
                0x48 => "Sensoco ADIO",
                0x51 | 0x61 => "Sensoco THU",
                0x80 => "Sensoco PulseCounter for E-Meter",
                _ => "Unknown Sensoco product",
            }
        });

        map.insert(mbus_manufacturer_id("SEN"), {
            match header.version {
                0x08 | 0x19 => "Sensus PolluCom E",
                0x0B => "Sensus PolluTherm",
                0x0E => "Sensus PolluStat E",
                _ => "Unknown Sensus product",
            }
        });

        map.insert(mbus_manufacturer_id("SON"), {
            match header.version {
                0x0D => "Sontex Supercal 531",
                _ => "Unknown Sontex product",
            }
        });

        map.insert(mbus_manufacturer_id("SPX"), {
            match header.version {
                0x31 | 0x34 => "Sensus PolluTherm",
                _ => "Unknown Sensus product",
            }
        });

        map.insert(mbus_manufacturer_id("SVM"), {
            match header.version {
                0x08 => "Elster F2 / Deltamess F2",
                0x09 => "Elster F4 / Kamstrup SVM F22",
                _ => "Unknown Elster/Kamstrup product",
            }
        });

        map.insert(mbus_manufacturer_id("TCH"), {
            match header.version {
                0x26 => "Techem m-bus S",
                0x40 => "Techem ultra S3",
                _ => "Unknown Techem product",
            }
        });

        map.insert(mbus_manufacturer_id("WZG"), {
            match header.version {
                0x03 => "Modularis ETW-EAX",
                _ => "Unknown Modularis product",
            }
        });

        map.insert(mbus_manufacturer_id("ZRM"), {
            match header.version {
                0x81 => "Minol Minocal C2",
                0x82 => "Minol Minocal WR3",
                _ => "Unknown Minol product",
            }
        });

        map
    };
}
```


### FIXED-LENGTH DATA RECORD FUNCTIONS

//   Value         Field Medium/Unit              Medium
// hexadecimal Bit 16  Bit 15    Bit 8  Bit 7
//     0        0       0         0     0         Other
//     1        0       0         0     1         Oil
//     2        0       0         1     0         Electricity
//     3        0       0         1     1         Gas
//     4        0       1         0     0         Heat
//     5        0       1         0     1         Steam
//     6        0       1         1     0         Hot Water
//     7        0       1         1     1         Water
//     8        1       0         0     0         H.C.A.
//     9        1       0         0     1         Reserved
//     A        1       0         1     0         Gas Mode 2
//     B        1       0         1     1         Heat Mode 2
//     C        1       1         0     0         Hot Water Mode 2
//     D        1       1         0     1         Water Mode 2
//     E        1       1         1     0         H.C.A. Mode 2
//     F        1       1         1     1         Reserved


```rust
use std::collections::HashMap;

lazy_static! {
    static ref FIXED_MEDIUM_TABLE: HashMap<u8, &'static str> = {
        let mut map = HashMap::new();
        map.insert(0b0000_0000, "Other");
        map.insert(0b0000_0001, "Oil");
        map.insert(0b0000_0010, "Electricity");
        map.insert(0b0000_0011, "Gas");
        map.insert(0b0000_0100, "Heat");
        map.insert(0b0000_0101, "Steam");
        map.insert(0b0000_0110, "Hot Water");
        map.insert(0b0000_0111, "Water");
        map.insert(0b1000_0000, "H.C.A.");
        map.insert(0b1000_0001, "Reserved");
        map.insert(0b1000_0010, "Gas Mode 2");
        map.insert(0b1000_0011, "Heat Mode 2");
        map.insert(0b1000_0100, "Hot Water Mode 2");
        map.insert(0b1000_0101, "Water Mode 2");
        map.insert(0b1000_0110, "H.C.A. Mode 2");
        map.insert(0b1000_0111, "Reserved");
        map
    };
}

pub fn mbus_data_fixed_medium(medium_unit: u8) -> &'static str {
    FIXED_MEDIUM_TABLE.get(&(medium_unit & 0b1111_1111)).unwrap_or(&"Unknown")
}
```


//------------------------------------------------------------------------------
//                        Hex code                            Hex code
//Unit                    share     Unit                      share
//              MSB..LSB                            MSB..LSB
//                       Byte 7/8                            Byte 7/8
// h,m,s         000000     00        MJ/h           100000     20
// D,M,Y         000001     01        MJ/h * 10      100001     21
//     Wh        000010     02        MJ/h * 100     100010     22
//     Wh * 10   000011     03        GJ/h           100011     23
//     Wh * 100  000100     04        GJ/h * 10      100100     24
//   kWh         000101     05        GJ/h * 100     100101     25
//  kWh   * 10   000110     06           ml          100110     26
//   kWh * 100   000111     07           ml * 10     100111     27
//   MWh         001000     08           ml * 100    101000     28
//   MWh * 10    001001     09            l          101001     29
//   MWh * 100   001010     0A            l * 10     101010     2A
//     kJ        001011     0B            l * 100    101011     2B
//     kJ * 10   001100     0C           m3          101100     2C
//     kJ * 100  001101     0D        m3 * 10        101101     2D
//     MJ        001110     0E        m3 * 100       101110     2E
//     MJ * 10   001111     0F        ml/h           101111     2F
//     MJ * 100  010000     10        ml/h * 10      110000     30
//     GJ        010001     11        ml/h * 100     110001     31
//     GJ * 10   010010     12         l/h           110010     32
//     GJ * 100  010011     13         l/h * 10      110011     33
//      W        010100     14         l/h * 100     110100     34
//      W * 10   010101     15        m3/h           110101     35
//      W * 100  010110     16       m3/h * 10       110110     36
//     kW        010111     17      m3/h * 100       110111     37
//     kW * 10   011000     18        °C* 10-3       111000     38
//     kW * 100  011001     19      units   for HCA  111001     39
//     MW        011010     1A    reserved           111010     3A
//     MW * 10   011011     1B    reserved           111011     3B
//     MW * 100  011100     1C    reserved           111100     3C
//  kJ/h         011101     1D    reserved           111101     3D
//  kJ/h * 10    011110     1E    same but historic  111110     3E
//  kJ/h * 100   011111     1F    without   units    111111     3F
//
//------------------------------------------------------------------------------
///
/// For fixed-length frames, get a string describing the unit of the data.
///


```rust
lazy_static! {
    static ref FIXED_UNIT_TABLE: HashMap<u8, &'static str> = {
        let mut map = HashMap::new();
        map.insert(0b0000_0000, "h,m,s");
        map.insert(0b0000_0001, "D,M,Y");
        map.insert(0b0000_0010, "Wh");
        map.insert(0b0000_0011, "10 Wh");
        map.insert(0b0000_0100, "100 Wh");
        map.insert(0b0000_0101, "kWh");
        map.insert(0b0000_0110, "10 kWh");
        map.insert(0b0000_0111, "100 kWh");
        map.insert(0b0000_1000, "MWh");
        map.insert(0b0000_1001, "10 MWh");
        map.insert(0b0000_1010, "100 MWh");
        map.insert(0b0000_1011, "kJ");
        map.insert(0b0000_1100, "10 kJ");
        map.insert(0b0000_1101, "100 kJ");
        map.insert(0b0000_1110, "MJ");
        map.insert(0b0000_1111, "10 MJ");
        map.insert(0b0001_0000, "100 MJ");
        map.insert(0b0001_0001, "GJ");
        map.insert(0b0001_0010, "10 GJ");
        map.insert(0b0001_0011, "100 GJ");
        map.insert(0b0001_0100, "W");
        map.insert(0b0001_0101, "10 W");
        map.insert(0b0001_0110, "100 W");
        map.insert(0b0001_0111, "kW");
        map.insert(0b0001_1000, "10 kW");
        map.insert(0b0001_1001, "100 kW");
        map.insert(0b0001_1010, "MW");
        map.insert(0b0001_1011, "10 MW");
        map.insert(0b0001_1100, "100 MW");
        map.insert(0b0001_1101, "kJ/h");
        map.insert(0b0001_1110, "10 kJ/h");
        map.insert(0b0001_1111, "100 kJ/h");
        map.insert(0b0010_0000, "MJ/h");
        map.insert(0b0010_0001, "10 MJ/h");
        map.insert(0b0010_0010, "100 MJ/h");
        map.insert(0b0010_0011, "GJ/h");
        map.insert(0b0010_0100, "10 GJ/h");
        map.insert(0b0010_0101, "100 GJ/h");
        map.insert(0b0010_0110, "ml");
        map.insert(0b0010_0111, "10 ml");
        map.insert(0b0010_1000, "100 ml");
        map.insert(0b0010_1001, "l");
        map.insert(0b0010_1010, "10 l");
        map.insert(0b0010_1011, "100 l");
        map.insert(0b0010_1100, "m^3");
        map.insert(0b0010_1101, "10 m^3");
        map.insert(0b0010_1110, "100 m^3");
        map.insert(0b0010_1111, "ml/h");
        map.insert(0b0011_0000, "10 ml/h");
        map.insert(0b0011_0001, "100 ml/h");
        map.insert(0b0011_0010, "l/h");
        map.insert(0b0011_0011, "10 l/h");
        map.insert(0b0011_0100, "100 l/h");
        map.insert(0b0011_0101, "m^3/h");
        map.insert(0b0011_0110, "10 m^3/h");
        map.insert(0b0011_0111, "100 m^3/h");
        map.insert(0b0011_1000, "1e-3 °C");
        map.insert(0b0011_1001, "units for HCA");
        map.insert(0b0011_1010, "reserved");
        map.insert(0b0011_1011, "reserved");
        map.insert(0b0011_1100, "reserved");
        map.insert(0b0011_1101, "reserved");
        map.insert(0b0011_1110, "reserved but historic");
        map.insert(0b0011_1111, "without units");
        map
    };
}

pub fn mbus_data_fixed_unit(medium_unit: u8) -> &'static str {
    FIXED_UNIT_TABLE.get(&medium_unit).unwrap_or(&"Unknown")
}
``


// Medium                                                              Code bin    Code hex
// Other                                                              0000 0000        00
// Oil                                                                0000 0001        01
// Electricity                                                        0000 0010        02
// Gas                                                                0000 0011        03
// Heat (Volume measured at return temperature: outlet)               0000 0100        04
// Steam                                                              0000 0101        05
// Hot Water                                                          0000 0110        06
// Water                                                              0000 0111        07
// Heat Cost Allocator.                                               0000 1000        08
// Compressed Air                                                     0000 1001        09
// Cooling load meter (Volume measured at return temperature: outlet) 0000 1010        0A
// Cooling load meter (Volume measured at flow temperature: inlet) ♣  0000 1011        0B
// Heat (Volume measured at flow temperature: inlet)                  0000 1100        0C
// Heat / Cooling load meter ♣                                        0000 1101        OD
// Bus / System                                                       0000 1110        0E
// Unknown Medium                                                     0000 1111        0F
// Irrigation Water (Non Drinkable)                                   0001 0000        10
// Water data logger                                                  0001 0001        11
// Gas data logger                                                    0001 0010        12
// Gas converter                                                      0001 0011        13
// Heat Value                                                         0001 0100        14
// Hot Water (>=90°C) (Non Drinkable)                                 0001 0101        15
// Cold Water                                                         0001 0110        16
// Dual Water                                                         0001 0111        17
// Pressure                                                           0001 1000        18
// A/D Converter                                                      0001 1001        19
// Smoke detector                                                     0001 1010        1A
// Room sensor (e.g. Temperature or Humidity)                         0001 1011        1B
// Gas detector                                                       0001 1100        1C
// Reserved for Sensors                                               .......... 1D to 1F
// Breaker (Electricity)                                              0010 0000        20
// Valve (Gas or Water)                                               0010 0001        21
// Reserved for Switching Units                                       .......... 22 to 24
// Customer Unit (Display)                                            0010 0101        25
// Reserved for End User Units                                        .......... 26 to 27
// Waste Water (Non Drinkable)                                        0010 1000        28
// Waste                                                              0010 1001        29
// Reserved for CO2                                                   0010 1010        2A
// Reserved for environmental meter                                   .......... 2B to 2F
// Service tool                                                       0011 0000        30
// Gateway                                                            0011 0001        31
// Unidirectional Repeater                                            0011 0010        32
// Bidirectional Repeater                                             0011 0011        33
// Reserved for System Units                                          .......... 34 to 35
// Radio Control Unit (System Side)                                   0011 0110        36
// Radio Control Unit (Meter Side)                                    0011 0111        37
// Bus Control Unit (Meter Side)                                      0011 1000        38
// Reserved for System Units                                          .......... 38 to 3F
// Reserved                                                           .......... 40 to FE
// Placeholder                                                        1111 1111        FF


```rust
use std::collections::HashMap;

lazy_static! {
    static ref VARIABLE_MEDIUM_TABLE: HashMap<u8, &'static str> = {
        let mut map = HashMap::new();
        map.insert(0x00, "Other");
        map.insert(0x01, "Oil");
        map.insert(0x02, "Electricity");
        map.insert(0x03, "Gas");
        map.insert(0x04, "Heat: Outlet");
        map.insert(0x05, "Steam");
        map.insert(0x06, "Warm water (30-90°C)");
        map.insert(0x07, "Water");
        map.insert(0x08, "Heat Cost Allocator");
        map.insert(0x09, "Compressed Air");
        map.insert(0x0A, "Cooling load meter: Outlet");
        map.insert(0x0B, "Cooling load meter: Inlet");
        map.insert(0x0C, "Heat: Inlet");
        map.insert(0x0D, "Heat / Cooling load meter");
        map.insert(0x0E, "Bus/System");
        map.insert(0x0F, "Unknown Medium");
        map.insert(0x10, "Irrigation Water");
        map.insert(0x11, "Water Logger");
        map.insert(0x12, "Gas Logger");
        map.insert(0x13, "Gas Converter");
        map.insert(0x14, "Calorific value");
        map.insert(0x15, "Hot water (>90°C)");
        map.insert(0x16, "Cold water");
        map.insert(0x17, "Dual water");
        map.insert(0x18, "Pressure");
        map.insert(0x19, "A/D Converter");
        map.insert(0x1A, "Smoke Detector");
        map.insert(0x1B, "Ambient Sensor");
        map.insert(0x1C, "Gas Detector");
        map.insert(0x20, "Breaker: Electricity");
        map.insert(0x21, "Valve: Gas or Water");
        map.insert(0x25, "Customer Unit: Display Device");
        map.insert(0x28, "Waste Water");
        map.insert(0x29, "Garbage");
        map.insert(0x30, "Service Unit");
        map.insert(0x36, "Radio Converter: System");
        map.insert(0x37, "Radio Converter: Meter");
        map.insert(0x38, "Bus Control Unit: Meter");
        map
    };
}

pub fn mbus_data_variable_medium_lookup(medium: u8) -> &'static str {
    VARIABLE_MEDIUM_TABLE.get(&medium).unwrap_or("Unknown (0x%.2x)")
}
```



```rust
lazy_static! {
    static ref VIF_UNIT_TABLE: HashMap<u8, &'static str> = {
        let mut map = HashMap::new();

        // E000 0nnn Energy 10(nnn-3) W
        for i in 0..8 {
            map.insert(0x00 + i, &format!("Energy ({}Wh)", mbus_unit_prefix(i - 3)));
        }

        // 0000 1nnn Energy 10(nnn)J (0.001kJ to 10000kJ)
        for i in 0..8 {
            map.insert(0x08 + i, &format!("Energy ({}J)", mbus_unit_prefix(i)));
        }

        // E001 1nnn Mass 10(nnn-3) kg 0.001kg to 10000kg
        for i in 0..8 {
            map.insert(0x18 + i, &format!("Mass ({}kg)", mbus_unit_prefix(i - 3)));
        }

        // E010 1nnn Power 10(nnn-3) W 0.001W to 10000W
        for i in 0..8 {
            map.insert(0x28 + i, &format!("Power ({}W)", mbus_unit_prefix(i - 3)));
        }

        // E011 0nnn Power 10(nnn) J/h 0.001kJ/h to 10000kJ/h
        for i in 0..8 {
            map.insert(0x30 + i, &format!("Power ({}J/h)", mbus_unit_prefix(i)));
        }

        // E001 0nnn Volume 10(nnn-6) m3 0.001l to 10000l
        for i in 0..8 {
            map.insert(0x10 + i, &format!("Volume ({}m^3)", mbus_unit_prefix(i - 6)));
        }

        // E011 1nnn Volume Flow 10(nnn-6) m3/h 0.001l/h to 10000l/
        for i in 0..8 {
            map.insert(0x38 + i, &format!("Volume flow ({}m^3/h)", mbus_unit_prefix(i - 6)));
        }

        // E100 0nnn Volume Flow ext. 10(nnn-7) m3/min 0.0001l/min to 1000l/min
        for i in 0..8 {
            map.insert(0x40 + i, &format!("Volume flow ({}m^3/min)", mbus_unit_prefix(i - 7)));
        }

        // E100 1nnn Volume Flow ext. 10(nnn-9) m3/s 0.001ml/s to 10000ml/
        for i in 0..8 {
            map.insert(0x48 + i, &format!("Volume flow ({}m^3/s)", mbus_unit_prefix(i - 9)));
        }

        // E101 0nnn Mass flow 10(nnn-3) kg/h 0.001kg/h to 10000kg/
        for i in 0..8 {
            map.insert(0x50 + i, &format!("Mass flow ({}kg/h)", mbus_unit_prefix(i - 3)));
        }

        // E101 10nn Flow Temperature 10(nn-3) °C 0.001°C to 1°C
        for i in 0..4 {
            map.insert(0x58 + i, &format!("Flow temperature ({}deg C)", mbus_unit_prefix(i - 3)));
        }

        // E101 11nn Return Temperature 10(nn-3) °C 0.001°C to 1°C
        for i in 0..4 {
            map.insert(0x5C + i, &format!("Return temperature ({}deg C)", mbus_unit_prefix(i - 3)));
        }

        // E110 10nn Pressure 10(nn-3) bar 1mbar to 1000mbar
        for i in 0..4 {
            map.insert(0x68 + i, &format!("Pressure ({}bar)", mbus_unit_prefix(i - 3)));
        }

        // E010 00nn On Time, E010 01nn Operating Time, E111 00nn Averaging Duration, E111 01nn Actuality Duration
        for i in 0..4 {
            let unit = match i {
                0 => "seconds",
                1 => "minutes",
                2 => "hours",
                3 => "days",
                _ => unreachable!(),
            };
            map.insert(0x20 + i, &format!("{} ({})", "On time", unit));
            map.insert(0x24 + i, &format!("{} ({})", "Operating time", unit));
            map.insert(0x70 + i, &format!("{} ({})", "Averaging Duration", unit));
            map.insert(0x74 + i, &format!("{} ({})", "Actuality Duration", unit));
        }

        // E110 110n Time Point
        map.insert(0x6C, "Time Point (date)");
        map.insert(0x6C + 1, "Time Point (time & date)");

        // E110 00nn Temperature Difference 10(nn-3)K (mK to K)
        for i in 0..4 {
            map.insert(0x60 + i, &format!("Temperature Difference ({}deg C)", mbus_unit_prefix(i - 3)));
        }

        // E110 01nn External Temperature 10(nn-3) °C 0.001°C to 1°C
        for i in 0..4 {
            map.insert(0x64 + i, &format!("External temperature ({}deg C)", mbus_unit_prefix(i - 3)));
        }

        map.insert(0x6E, "Units for H.C.A.");
        map.insert(0x6F, "Reserved");
        map.insert(0x7C, "Custom VIF");
        map.insert(0x78, "Fabrication number");
        map.insert(0x7A, "Bus Address");
        map.insert(0x7F, "Manufacturer specific");
        map.insert(0xFF, "Manufacturer specific");

        map
    };
}

pub fn mbus_vif_unit_lookup(vif: u8) -> &'static str {
    VIF_UNIT_TABLE.get(&(vif & 0xF7)).unwrap_or(&"Unknown (VIF=0x%.2X)")
}
```




## Nom-based Implementations

Function Name               Rust unit       Rationale
mbus_parse                  frame.rs        Use nom to parse the different types of M-Bus frames (ACK, Short, Control, Long).
mbus_data_fixed_parse       record.rs       Use nom to parse the fixed-length data records.
mbus_data_variable_parse    record.rs       Use nom to parse the variable-length data records.
mbus_frame_data_parse       frame.rs        Use nom to parse the different types of data (error, fixed, variable) in the M-Bus frames.
mbus_data_record_decode     data.rs         Use nom to decode the data records based on the DIF and VIF/VIFE information.
mbus_vib_unit_lookup_fd     vif.rs          Use nom to parse the VIF extension codes and their corresponding unit descriptions.
