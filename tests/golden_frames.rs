use mbus_rs::mbus::frame::parse_frame;
use mbus_rs::{MBusFrame, MBusFrameType};
use nom::IResult;

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

const EXAMPLE_DATA_01_HEX: &str = "6831316808017245585703B40534049E0027B60306F934150315C6004D052E00000000053D00000000055B22F32642055FC7DA0D42FA16";

const EDC_HEX: &str = "68AEAE682801729508121183140204170000008400863B230000008400863CD10100008440863B000000008440863C0000000085005B2B4BAC4185005F20D7AC4185405B0000B84285405F0000B84285003B8400353F85403B0000000095003B95CFB24395403B0000000085002B0000000085402B0000000095002BD39F904695402B00000000046D190F8A1784007C0143F30D000084407C01439D01000084007C01630100000084407C0163010000000F2F16";

const APPLICATION_BUSY_HEX: &str = "68040468080170088116";

const EFE_ENGELMANN_HEX: &str = "68A1A16808007245330824C5140004662700000478917B6F01046D172ECC13041500000000441500000000840115000000000406000000004406000000008401060000000084100600000000C410060000000084110600000000426CBF1C026CDF1C8420060000000084300600000000043B00000000143B19000000042B00000000142B0B000000025B1600025F150004610900000002230C0201FD17000490280B000000EB16";

const ELS_ELSTER_HEX: &str = "686868680800725139494493152F04A17000000C06000000008C1006000000008C2013000000000C13000000003C2BBDEBDDDD3B3BBDEBDD0A5A27020A5E26020A6201000A273007046D090DCD134C06000000004C1300000000CC100600000000CC201300000000426CBF154016";

#[test]
fn test_example_data_01() {
    let data = hex_to_bytes(EXAMPLE_DATA_01_HEX);
    let result: IResult<&[u8], MBusFrame> = parse_frame(&data);
    match result {
        Ok((_remaining, frame)) => {
            assert_eq!(frame.frame_type, MBusFrameType::Long);
            // Add more checks
        }
        Err(e) => panic!("Failed to parse: {:?}", e),
    }
}

#[test]
fn test_edc() {
    let data = hex_to_bytes(EDC_HEX);
    let result: IResult<&[u8], MBusFrame> = parse_frame(&data);
    match result {
        Ok((_remaining, frame)) => {
            assert_eq!(frame.frame_type, MBusFrameType::Long);
            // Add more assertions based on expected data
        }
        Err(e) => panic!("Failed to parse: {:?}", e),
    }
}

#[test]
fn test_application_busy_error() {
    let data = hex_to_bytes(APPLICATION_BUSY_HEX);
    let result: IResult<&[u8], MBusFrame> = parse_frame(&data);
    match result {
        Ok((_remaining, frame)) => {
            assert_eq!(frame.control_information, 0x70); // Error general
            assert_eq!(frame.data, vec![0x08]); // Application busy
        }
        Err(e) => panic!("Failed to parse: {:?}", e),
    }
}

#[test]
fn test_efe_engelmann() {
    let data = hex_to_bytes(EFE_ENGELMANN_HEX);
    let result: IResult<&[u8], MBusFrame> = parse_frame(&data);
    match result {
        Ok((_remaining, frame)) => {
            assert_eq!(frame.frame_type, MBusFrameType::Long);
            // Add more checks as needed
        }
        Err(e) => panic!("Failed to parse: {:?}", e),
    }
}

#[test]
fn test_els_elster() {
    let data = hex_to_bytes(ELS_ELSTER_HEX);
    let result: IResult<&[u8], MBusFrame> = parse_frame(&data);
    match result {
        Ok((_remaining, frame)) => {
            assert_eq!(frame.frame_type, MBusFrameType::Long);
            // Add more checks as needed
        }
        Err(e) => panic!("Failed to parse: {:?}", e),
    }
}
