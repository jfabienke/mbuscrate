//! SIMD/NEON runtime feature detection and optimized implementations
//!
//! This module provides runtime CPU feature detection to select the best
//! available implementation for performance-critical operations.

use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

// Feature detection flags
static NEON_AVAILABLE: AtomicBool = AtomicBool::new(false);
static SSE2_AVAILABLE: AtomicBool = AtomicBool::new(false);
static AVX2_AVAILABLE: AtomicBool = AtomicBool::new(false);
static INIT: Once = Once::new();

/// Initialize CPU feature detection
pub fn init_cpu_features() {
    INIT.call_once(|| {
        #[cfg(target_arch = "aarch64")]
        {
            // ARM64 always has NEON
            NEON_AVAILABLE.store(true, Ordering::Relaxed);
            log::info!("NEON support detected: enabled hardware acceleration");

            // Check for additional ARM features
            if std::arch::is_aarch64_feature_detected!("crc") {
                log::info!("ARM CRC instructions detected");
            }

            // Detect Raspberry Pi model if possible
            detect_raspberry_pi_model();
        }

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if is_x86_feature_detected!("sse2") {
                SSE2_AVAILABLE.store(true, Ordering::Relaxed);
                log::info!("SSE2 support detected: enabled SIMD acceleration");
            }

            if is_x86_feature_detected!("avx2") {
                AVX2_AVAILABLE.store(true, Ordering::Relaxed);
                log::info!("AVX2 support detected: enabled advanced SIMD acceleration");
            }
        }
    });
}

/// Detect Raspberry Pi model for optimizations
#[cfg(target_arch = "aarch64")]
fn detect_raspberry_pi_model() {
    // Try to read CPU info to detect Pi model
    if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
        if cpuinfo.contains("BCM2711") || cpuinfo.contains("Cortex-A72") {
            log::info!("Detected Raspberry Pi 4 (Cortex-A72)");
        } else if cpuinfo.contains("BCM2712") || cpuinfo.contains("Cortex-A76") {
            log::info!("Detected Raspberry Pi 5 (Cortex-A76) - enhanced NEON performance");
        }
    }
}

#[cfg(not(target_arch = "aarch64"))]
fn detect_raspberry_pi_model() {
    // No-op on non-ARM platforms
}

/// Check if NEON is available
#[inline]
pub fn has_neon() -> bool {
    init_cpu_features();
    NEON_AVAILABLE.load(Ordering::Relaxed)
}

/// Check if SSE2 is available
#[inline]
pub fn has_sse2() -> bool {
    init_cpu_features();
    SSE2_AVAILABLE.load(Ordering::Relaxed)
}

/// Check if AVX2 is available
#[inline]
pub fn has_avx2() -> bool {
    init_cpu_features();
    AVX2_AVAILABLE.load(Ordering::Relaxed)
}

/// Optimized checksum calculation with runtime feature detection
#[inline]
pub fn calculate_checksum_optimized(data: &[u8]) -> u8 {
    #[cfg(target_arch = "aarch64")]
    {
        if has_neon() && data.len() >= 16 {
            return unsafe { calculate_checksum_neon(data) };
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if has_avx2() && data.len() >= 32 {
            return unsafe { calculate_checksum_avx2(data) };
        } else if has_sse2() && data.len() >= 16 {
            return unsafe { calculate_checksum_sse2(data) };
        }
    }

    // Fallback to scalar implementation
    calculate_checksum_scalar(data)
}

/// Scalar implementation (fallback)
#[inline(always)]
fn calculate_checksum_scalar(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// NEON implementation for ARM64 (optimized for Raspberry Pi 4/5)
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn calculate_checksum_neon(data: &[u8]) -> u8 {
    use std::arch::aarch64::*;

    let mut sum = vdupq_n_u32(0);
    let mut i = 0;

    // Process 64 bytes at a time for better cache utilization on Pi 4/5
    // Both Pi 4 (Cortex-A72) and Pi 5 (Cortex-A76) benefit from larger chunks
    while i + 64 <= data.len() {
        // Load 4x16 bytes
        let chunk1 = vld1q_u8(data.as_ptr().add(i));
        let chunk2 = vld1q_u8(data.as_ptr().add(i + 16));
        let chunk3 = vld1q_u8(data.as_ptr().add(i + 32));
        let chunk4 = vld1q_u8(data.as_ptr().add(i + 48));

        // Process chunk1
        let low1 = vmovl_u8(vget_low_u8(chunk1));
        let high1 = vmovl_u8(vget_high_u8(chunk1));
        let low_wide1 = vaddl_u16(vget_low_u16(low1), vget_high_u16(low1));
        let high_wide1 = vaddl_u16(vget_low_u16(high1), vget_high_u16(high1));

        // Process chunk2
        let low2 = vmovl_u8(vget_low_u8(chunk2));
        let high2 = vmovl_u8(vget_high_u8(chunk2));
        let low_wide2 = vaddl_u16(vget_low_u16(low2), vget_high_u16(low2));
        let high_wide2 = vaddl_u16(vget_low_u16(high2), vget_high_u16(high2));

        // Process chunk3
        let low3 = vmovl_u8(vget_low_u8(chunk3));
        let high3 = vmovl_u8(vget_high_u8(chunk3));
        let low_wide3 = vaddl_u16(vget_low_u16(low3), vget_high_u16(low3));
        let high_wide3 = vaddl_u16(vget_low_u16(high3), vget_high_u16(high3));

        // Process chunk4
        let low4 = vmovl_u8(vget_low_u8(chunk4));
        let high4 = vmovl_u8(vget_high_u8(chunk4));
        let low_wide4 = vaddl_u16(vget_low_u16(low4), vget_high_u16(low4));
        let high_wide4 = vaddl_u16(vget_low_u16(high4), vget_high_u16(high4));

        // Accumulate all
        sum = vaddq_u32(sum, low_wide1);
        sum = vaddq_u32(sum, high_wide1);
        sum = vaddq_u32(sum, low_wide2);
        sum = vaddq_u32(sum, high_wide2);
        sum = vaddq_u32(sum, low_wide3);
        sum = vaddq_u32(sum, high_wide3);
        sum = vaddq_u32(sum, low_wide4);
        sum = vaddq_u32(sum, high_wide4);

        i += 64;
    }

    // Process 16 bytes at a time
    while i + 16 <= data.len() {
        let chunk = vld1q_u8(data.as_ptr().add(i));

        // Widen to 16-bit
        let low = vmovl_u8(vget_low_u8(chunk));
        let high = vmovl_u8(vget_high_u8(chunk));

        // Widen to 32-bit and accumulate
        let low_wide = vaddl_u16(vget_low_u16(low), vget_high_u16(low));
        let high_wide = vaddl_u16(vget_low_u16(high), vget_high_u16(high));

        sum = vaddq_u32(sum, low_wide);
        sum = vaddq_u32(sum, high_wide);

        i += 16;
    }

    // Sum all lanes
    let sum64 = vaddlvq_u32(sum);
    let mut result = (sum64 & 0xFF) as u8;

    // Handle remaining bytes
    while i < data.len() {
        result = result.wrapping_add(data[i]);
        i += 1;
    }

    result
}

/// SSE2 implementation for x86/x86_64
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
unsafe fn calculate_checksum_sse2(data: &[u8]) -> u8 {
    use std::arch::x86_64::*;

    let mut sum = _mm_setzero_si128();
    let mut i = 0;

    while i + 16 <= data.len() {
        let chunk = _mm_loadu_si128(data.as_ptr().add(i) as *const __m128i);

        let zero = _mm_setzero_si128();
        let low = _mm_unpacklo_epi8(chunk, zero);
        let high = _mm_unpackhi_epi8(chunk, zero);

        let low_low = _mm_unpacklo_epi16(low, zero);
        let low_high = _mm_unpackhi_epi16(low, zero);
        let high_low = _mm_unpacklo_epi16(high, zero);
        let high_high = _mm_unpackhi_epi16(high, zero);

        sum = _mm_add_epi32(sum, low_low);
        sum = _mm_add_epi32(sum, low_high);
        sum = _mm_add_epi32(sum, high_low);
        sum = _mm_add_epi32(sum, high_high);

        i += 16;
    }

    let mut result_array = [0u32; 4];
    _mm_storeu_si128(result_array.as_mut_ptr() as *mut __m128i, sum);
    let mut result = (result_array.iter().sum::<u32>() & 0xFF) as u8;

    while i < data.len() {
        result = result.wrapping_add(data[i]);
        i += 1;
    }

    result
}

/// AVX2 implementation for x86/x86_64
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn calculate_checksum_avx2(data: &[u8]) -> u8 {
    use std::arch::x86_64::*;

    let mut sum = _mm256_setzero_si256();
    let mut i = 0;

    // Process 32 bytes at a time
    while i + 32 <= data.len() {
        let chunk = _mm256_loadu_si256(data.as_ptr().add(i) as *const __m256i);

        // Split into two 128-bit halves
        let low = _mm256_extracti128_si256(chunk, 0);
        let high = _mm256_extracti128_si256(chunk, 1);

        // Process each half
        let zero = _mm_setzero_si128();

        // Low half
        let low_low = _mm_unpacklo_epi8(low, zero);
        let low_high = _mm_unpackhi_epi8(low, zero);

        // High half
        let high_low = _mm_unpacklo_epi8(high, zero);
        let high_high = _mm_unpackhi_epi8(high, zero);

        // Combine and widen to 32-bit
        let combined_low = _mm256_set_m128i(high_low, low_low);
        let combined_high = _mm256_set_m128i(high_high, low_high);

        let zero256 = _mm256_setzero_si256();
        let wide_low = _mm256_unpacklo_epi16(combined_low, zero256);
        let wide_high = _mm256_unpackhi_epi16(combined_low, zero256);

        sum = _mm256_add_epi32(sum, wide_low);
        sum = _mm256_add_epi32(sum, wide_high);

        let wide_low2 = _mm256_unpacklo_epi16(combined_high, zero256);
        let wide_high2 = _mm256_unpackhi_epi16(combined_high, zero256);

        sum = _mm256_add_epi32(sum, wide_low2);
        sum = _mm256_add_epi32(sum, wide_high2);

        i += 32;
    }

    // Extract and sum all lanes
    let mut result_array = [0u32; 8];
    _mm256_storeu_si256(result_array.as_mut_ptr() as *mut __m256i, sum);
    let mut result = (result_array.iter().sum::<u32>() & 0xFF) as u8;

    // Handle remaining bytes
    while i < data.len() {
        result = result.wrapping_add(data[i]);
        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_consistency() {
        let test_data = vec![0x42u8; 1024];

        let scalar_result = calculate_checksum_scalar(&test_data);
        let optimized_result = calculate_checksum_optimized(&test_data);

        assert_eq!(scalar_result, optimized_result,
                   "SIMD implementation should match scalar implementation");
    }

    #[test]
    fn test_checksum_edge_cases() {
        // Empty data
        assert_eq!(calculate_checksum_optimized(&[]), 0);

        // Single byte
        assert_eq!(calculate_checksum_optimized(&[0x42]), 0x42);

        // Small data (less than SIMD threshold)
        let small_data = vec![1, 2, 3, 4, 5];
        let expected = small_data.iter().sum::<u8>();
        assert_eq!(calculate_checksum_optimized(&small_data), expected);
    }

    #[test]
    fn test_feature_detection() {
        init_cpu_features();

        #[cfg(target_arch = "aarch64")]
        {
            assert!(has_neon(), "NEON should be available on ARM64");
        }

        // Just verify the functions don't panic
        let _ = has_sse2();
        let _ = has_avx2();
    }
}