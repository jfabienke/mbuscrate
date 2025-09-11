#![allow(unused_imports)]
use mbus_rs::mbus::frame::parse_frame;

fn main() {
    let hex_data = "68 03 03 68 53 01 00 54 16";
    let bytes = hex::decode(hex_data.replace(" ", "")).unwrap();
    let (_, frame) = parse_frame(&bytes).unwrap();
    println!("{frame:?}");
}
