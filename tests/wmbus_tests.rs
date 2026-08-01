use mbus_rs::error::MBusError;
use mbus_rs::wmbus::encryption::WMBusEncryption;

#[tokio::test]
async fn test_wmbus_encrypt_unimplemented() {
    let encryption = WMBusEncryption::new();
    let result = encryption.encrypt(&[0u8; 4]);
    assert!(matches!(result, Err(MBusError::Other(_))));
}

#[tokio::test]
async fn test_wmbus_decrypt_unimplemented() {
    let encryption = WMBusEncryption::new();
    let result = encryption.decrypt(&[0u8; 4]);
    assert!(matches!(result, Err(MBusError::Other(_))));
}

// NOTE: two placeholder tests for `wmbus_protocol` were removed here. They asserted
// nothing (`assert!(true)`) and referenced src/wmbus/wmbus_protocol.rs, which is a
// zero-byte file declared in no module and therefore never compiled. Reinstate real
// tests alongside an actual implementation.
