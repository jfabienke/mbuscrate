//! NIST SHA-1 and HMAC-SHA1 test vectors
//!
//! This test suite validates the SHA-1 and HMAC-SHA1 implementations against
//! official NIST test vectors to ensure cryptographic correctness.

use mbus_rs::wmbus::sha_hardware::{calculate_sha1, calculate_hmac_sha1};

/// NIST SHA-1 test vectors from FIPS 180-4
#[cfg(test)]
mod sha1_nist_vectors {
    use super::*;

    #[test]
    fn test_sha1_empty_string() {
        // NIST vector: empty string
        let input = b"";
        let expected = [
            0xda, 0x39, 0xa3, 0xee, 0x5e, 0x6b, 0x4b, 0x0d,
            0x32, 0x55, 0xbf, 0xef, 0x95, 0x60, 0x18, 0x90,
            0xaf, 0xd8, 0x07, 0x09,
        ];
        let result = calculate_sha1(input);
        assert_eq!(result, expected, "SHA-1 empty string test failed");
    }

    #[test]
    fn test_sha1_abc() {
        // NIST vector: "abc"
        let input = b"abc";
        let expected = [
            0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a,
            0xba, 0x3e, 0x25, 0x71, 0x78, 0x50, 0xc2, 0x6c,
            0x9c, 0xd0, 0xd8, 0x9d,
        ];
        let result = calculate_sha1(input);
        assert_eq!(result, expected, "SHA-1 'abc' test failed");
    }

    #[test]
    fn test_sha1_long_message() {
        // NIST vector: "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        let expected = [
            0x84, 0x98, 0x3e, 0x44, 0x1c, 0x3b, 0xd2, 0x6e,
            0xba, 0xae, 0x4a, 0xa1, 0xf9, 0x51, 0x29, 0xe5,
            0xe5, 0x46, 0x70, 0xf1,
        ];
        let result = calculate_sha1(input);
        assert_eq!(result, expected, "SHA-1 long message test failed");
    }

    #[test]
    fn test_sha1_million_a() {
        // NIST vector: 1 million 'a' characters
        let input = vec![b'a'; 1_000_000];
        let expected = [
            0x34, 0xaa, 0x97, 0x3c, 0xd4, 0xc4, 0xda, 0xa4,
            0xf6, 0x1e, 0xeb, 0x2b, 0xdb, 0xad, 0x27, 0x31,
            0x65, 0x34, 0x01, 0x6f,
        ];
        let result = calculate_sha1(&input);
        assert_eq!(result, expected, "SHA-1 million 'a' test failed");
    }

    #[test]
    fn test_sha1_boundary_cases() {
        // Test exactly 55 bytes (just under padding boundary)
        let input_55 = vec![b'a'; 55];
        let result_55 = calculate_sha1(&input_55);
        assert_eq!(result_55.len(), 20, "SHA-1 result should be 20 bytes");

        // Test exactly 56 bytes (at padding boundary)
        let input_56 = vec![b'a'; 56];
        let result_56 = calculate_sha1(&input_56);
        assert_eq!(result_56.len(), 20, "SHA-1 result should be 20 bytes");

        // Test exactly 64 bytes (one block)
        let input_64 = vec![b'a'; 64];
        let result_64 = calculate_sha1(&input_64);
        assert_eq!(result_64.len(), 20, "SHA-1 result should be 20 bytes");

        // Ensure they're all different
        assert_ne!(result_55, result_56);
        assert_ne!(result_56, result_64);
        assert_ne!(result_55, result_64);
    }

    #[test]
    fn test_sha1_bit_patterns() {
        // Test various bit patterns
        let patterns = [
            vec![0x00; 64], // All zeros
            vec![0xFF; 64], // All ones
            (0..64).map(|i| i as u8).collect::<Vec<u8>>(), // Incrementing
            (0..64).map(|i| (i % 2) as u8 * 0xFF).collect::<Vec<u8>>(), // Alternating
        ];

        for (i, pattern) in patterns.iter().enumerate() {
            let result = calculate_sha1(pattern);
            assert_eq!(result.len(), 20, "Pattern {} should produce 20-byte hash", i);
            
            // Verify deterministic
            let result2 = calculate_sha1(pattern);
            assert_eq!(result, result2, "Pattern {} should be deterministic", i);
        }
    }
}

/// NIST HMAC-SHA1 test vectors from RFC 2202
#[cfg(test)]
mod hmac_sha1_nist_vectors {
    use super::*;

    #[test]
    fn test_hmac_sha1_rfc2202_case1() {
        // RFC 2202 Test Case 1
        let key = vec![0x0b; 20];
        let data = b"Hi There";
        let expected = [
            0xb6, 0x17, 0x31, 0x86, 0x55, 0x05, 0x72, 0x64,
            0xe2, 0x8b, 0xc0, 0xb6, 0xfb, 0x37, 0x8c, 0x8e,
            0xf1, 0x46, 0xbe, 0x00,
        ];
        let result = calculate_hmac_sha1(&key, data);
        assert_eq!(result, expected, "HMAC-SHA1 RFC 2202 Case 1 failed");
    }

    #[test]
    fn test_hmac_sha1_rfc2202_case2() {
        // RFC 2202 Test Case 2
        let key = b"Jefe";
        let data = b"what do ya want for nothing?";
        let expected = [
            0xef, 0xfc, 0xdf, 0x6a, 0xe5, 0xeb, 0x2f, 0xa2,
            0xd2, 0x74, 0x16, 0xd5, 0xf1, 0x84, 0xdf, 0x9c,
            0x25, 0x9a, 0x7c, 0x79,
        ];
        let result = calculate_hmac_sha1(key, data);
        assert_eq!(result, expected, "HMAC-SHA1 RFC 2202 Case 2 failed");
    }

    #[test]
    fn test_hmac_sha1_rfc2202_case3() {
        // RFC 2202 Test Case 3
        let key = vec![0xaa; 20];
        let data = vec![0xdd; 50];
        let expected = [
            0x12, 0x5d, 0x73, 0x42, 0xb9, 0xac, 0x11, 0xcd,
            0x91, 0xa3, 0x9a, 0xf4, 0x8a, 0xa1, 0x7b, 0x4f,
            0x63, 0xf1, 0x75, 0xd3,
        ];
        let result = calculate_hmac_sha1(&key, &data);
        assert_eq!(result, expected, "HMAC-SHA1 RFC 2202 Case 3 failed");
    }

    #[test]
    fn test_hmac_sha1_rfc2202_case4() {
        // RFC 2202 Test Case 4
        let key = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
            0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19,
        ];
        let data = vec![0xcd; 50];
        let expected = [
            0x4c, 0x90, 0x07, 0xf4, 0x02, 0x62, 0x50, 0xc6,
            0xbc, 0x84, 0x14, 0xf9, 0xbf, 0x50, 0xc8, 0x6c,
            0x2d, 0x72, 0x35, 0xda,
        ];
        let result = calculate_hmac_sha1(&key, &data);
        assert_eq!(result, expected, "HMAC-SHA1 RFC 2202 Case 4 failed");
    }

    #[test]
    fn test_hmac_sha1_rfc2202_case5() {
        // RFC 2202 Test Case 5
        let key = vec![0x0c; 20];
        let data = b"Test With Truncation";
        let expected = [
            0x4c, 0x1a, 0x03, 0x42, 0x4b, 0x55, 0xe0, 0x7f,
            0xe7, 0xf2, 0x7b, 0xe1, 0xd5, 0x8b, 0xb9, 0x32,
            0x4a, 0x9a, 0x5a, 0x04,
        ];
        let result = calculate_hmac_sha1(&key, data);
        assert_eq!(result, expected, "HMAC-SHA1 RFC 2202 Case 5 failed");
    }

    #[test]
    fn test_hmac_sha1_rfc2202_case6() {
        // RFC 2202 Test Case 6
        let key = vec![0xaa; 80];
        let data = b"Test Using Larger Than Block-Size Key - Hash Key First";
        let expected = [
            0xaa, 0x4a, 0xe5, 0xe1, 0x52, 0x72, 0xd0, 0x0e,
            0x95, 0x70, 0x56, 0x37, 0xce, 0x8a, 0x3b, 0x55,
            0xed, 0x40, 0x21, 0x12,
        ];
        let result = calculate_hmac_sha1(&key, data);
        assert_eq!(result, expected, "HMAC-SHA1 RFC 2202 Case 6 failed");
    }

    #[test]
    fn test_hmac_sha1_rfc2202_case7() {
        // RFC 2202 Test Case 7
        let key = vec![0xaa; 80];
        let data = b"Test Using Larger Than Block-Size Key and Larger Than One Block-Size Data";
        let expected = [
            0xe8, 0xe9, 0x9d, 0x0f, 0x45, 0x23, 0x7d, 0x78,
            0x6d, 0x6b, 0xba, 0xa7, 0x96, 0x5c, 0x78, 0x08,
            0xbb, 0xff, 0x1a, 0x91,
        ];
        let result = calculate_hmac_sha1(&key, data);
        assert_eq!(result, expected, "HMAC-SHA1 RFC 2202 Case 7 failed");
    }

    #[test]
    fn test_hmac_sha1_edge_cases() {
        // Empty key and data
        let result1 = calculate_hmac_sha1(b"", b"");
        assert_eq!(result1.len(), 20, "HMAC with empty key/data should be 20 bytes");

        // Empty key, non-empty data
        let result2 = calculate_hmac_sha1(b"", b"test");
        assert_eq!(result2.len(), 20, "HMAC with empty key should be 20 bytes");

        // Non-empty key, empty data
        let result3 = calculate_hmac_sha1(b"key", b"");
        assert_eq!(result3.len(), 20, "HMAC with empty data should be 20 bytes");

        // All should be different
        assert_ne!(result1, result2);
        assert_ne!(result2, result3);
        assert_ne!(result1, result3);
    }

    #[test]
    fn test_hmac_sha1_key_sizes() {
        let data = b"test data";
        
        // Test various key sizes
        let key_sizes = [1, 16, 32, 63, 64, 65, 128, 256];
        let mut results = Vec::new();
        
        for &size in &key_sizes {
            let key = vec![0x42u8; size];
            let result = calculate_hmac_sha1(&key, data);
            assert_eq!(result.len(), 20, "HMAC result should be 20 bytes for key size {}", size);
            results.push(result);
        }
        
        // All results should be different
        for i in 0..results.len() {
            for j in i + 1..results.len() {
                assert_ne!(results[i], results[j], 
                    "HMAC results should differ for key sizes {} and {}", 
                    key_sizes[i], key_sizes[j]);
            }
        }
    }
}

/// Regression tests for wM-Bus specific scenarios
#[cfg(test)]
mod wmbus_regression_tests {
    use super::*;

    #[test]
    fn test_qundis_authentication_scenario() {
        // Simulate Qundis 3-step authentication
        let device_key = b"Qundis_Test_Key_";
        let challenge = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        
        // Step 1: Prepare authentication message
        let mut auth_msg = Vec::new();
        auth_msg.extend_from_slice(&challenge);
        auth_msg.extend_from_slice(&[0x00; 8]); // Padding
        
        // Step 2: Calculate HMAC-SHA1
        let hmac_result = calculate_hmac_sha1(device_key, &auth_msg);
        
        // Verify result properties
        assert_eq!(hmac_result.len(), 20, "HMAC result should be 20 bytes");
        assert_ne!(hmac_result, [0u8; 20], "HMAC result should not be all zeros");
        
        // Test reproducibility
        let hmac_result2 = calculate_hmac_sha1(device_key, &auth_msg);
        assert_eq!(hmac_result, hmac_result2, "HMAC should be reproducible");
    }

    #[test]
    fn test_wmbus_frame_integrity() {
        // Test SHA-1 on typical wM-Bus frame data
        let frame_data = [
            0x68, 0x1F, 0x1F, 0x68, // Start bytes and length
            0x08, 0x01, 0x72,       // Control, address, CI
            0x78, 0x56, 0x34, 0x12, // Manufacturer
            0x01, 0x00, 0x00, 0x00, // Version, type
            0x05, 0x14, 0x00,       // Status, signature
            // Data records...
            0x0C, 0x13, 0x12, 0x34, 0x56, 0x78, // Energy
            0x02, 0x59, 0x12, 0x34,             // Volume
            0x16, // Checksum (placeholder)
        ];
        
        let hash = calculate_sha1(&frame_data);
        assert_eq!(hash.len(), 20, "Frame hash should be 20 bytes");
        
        // Verify deterministic
        let hash2 = calculate_sha1(&frame_data);
        assert_eq!(hash, hash2, "Frame hash should be deterministic");
        
        // Verify sensitivity to changes
        let mut modified_frame = frame_data;
        modified_frame[5] ^= 0x01; // Flip one bit
        let hash_modified = calculate_sha1(&modified_frame);
        assert_ne!(hash, hash_modified, "Hash should change with frame modification");
    }

    #[test]
    fn test_performance_regression() {
        // Test that hardware acceleration doesn't introduce correctness issues
        let large_data = vec![0x5A; 10000]; // 10KB of data
        let key = b"performance_test_key_1234567890";
        
        // Multiple iterations to catch intermittent issues
        let mut results = Vec::new();
        for _ in 0..10 {
            let sha_result = calculate_sha1(&large_data);
            let hmac_result = calculate_hmac_sha1(key, &large_data);
            
            assert_eq!(sha_result.len(), 20);
            assert_eq!(hmac_result.len(), 20);
            
            results.push((sha_result, hmac_result));
        }
        
        // All results should be identical
        let (first_sha, first_hmac) = &results[0];
        for (sha, hmac) in &results[1..] {
            assert_eq!(sha, first_sha, "SHA-1 results should be consistent");
            assert_eq!(hmac, first_hmac, "HMAC-SHA1 results should be consistent");
        }
    }
}

/// Cross-platform compatibility tests
#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn test_cross_platform_consistency() {
        // Test vectors that should produce same results on all platforms
        let test_cases = [
            (b"" as &[u8], b"" as &[u8]),
            (b"key", b"data"),
            (b"a", &vec![b'b'; 1000]),
            (&vec![0xFF; 100], &vec![0x00; 100]),
        ];
        
        for (key, data) in test_cases {
            let sha_result = calculate_sha1(data);
            let hmac_result = calculate_hmac_sha1(key, data);
            
            // Verify basic properties
            assert_eq!(sha_result.len(), 20, "SHA-1 should always be 20 bytes");
            assert_eq!(hmac_result.len(), 20, "HMAC-SHA1 should always be 20 bytes");
            
            // Verify non-zero (except for very specific edge cases)
            if !data.is_empty() || !key.is_empty() {
                assert_ne!(sha_result, [0u8; 20], "SHA-1 should not be all zeros");
                assert_ne!(hmac_result, [0u8; 20], "HMAC-SHA1 should not be all zeros");
            }
        }
    }
}
