//! # IoBuffer - Efficient Streaming Buffer
//!
//! This module provides an appendable/circular buffer implementation optimized
//! for streaming data scenarios common in M-Bus and radio communication.
//! Implements a circular buffer optimized for async I/O operations and frame processing.
//!
//! ## Features
//!
//! - Efficient append/consume operations using VecDeque
//! - Capacity management with hints for partial packets
//! - Async-friendly non-blocking operations
//! - Memory-efficient for streaming scenarios
//! - Compatible with both sync and async code
//!
//! ## Usage
//!
//! ```rust
//! use mbus_rs::util::IoBuffer;
//!
//! let mut buffer = IoBuffer::new();
//! buffer.write(&[0x01, 0x02, 0x03]).unwrap();
//!
//! let data = buffer.consume(2);
//! assert_eq!(data, vec![0x01, 0x02]);
//! ```

use std::collections::VecDeque;
use thiserror::Error;

/// Errors that can occur during IoBuffer operations
#[derive(Error, Debug, Clone, PartialEq)]
pub enum IoBufferError {
    #[error("Insufficient capacity: requested {requested}, available {available}")]
    InsufficientCapacity { requested: usize, available: usize },

    #[error("Buffer empty: no data available")]
    BufferEmpty,

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Capacity limit exceeded: {limit}")]
    CapacityExceeded { limit: usize },
}

/// Efficient streaming buffer for M-Bus and radio communication
///
/// IoBuffer provides an efficient circular buffer implementation using VecDeque
/// that's optimized for streaming data scenarios. It supports both blocking and
/// non-blocking operations, making it suitable for async contexts.
#[derive(Debug, Clone)]
pub struct IoBuffer {
    /// Internal storage using VecDeque for efficient front/back operations
    data: VecDeque<u8>,
    /// Maximum capacity limit (None = unlimited)
    capacity_limit: Option<usize>,
    /// Statistics for monitoring
    bytes_written: u64,
    bytes_consumed: u64,
}

impl IoBuffer {
    /// Default capacity limit for safety (10MB)
    pub const DEFAULT_CAPACITY_LIMIT: usize = 10 * 1024 * 1024;

    /// Create a new IoBuffer with default capacity
    pub fn new() -> Self {
        Self {
            data: VecDeque::new(),
            capacity_limit: Some(Self::DEFAULT_CAPACITY_LIMIT),
            bytes_written: 0,
            bytes_consumed: 0,
        }
    }

    /// Create a new IoBuffer with specified initial capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(capacity),
            capacity_limit: Some(Self::DEFAULT_CAPACITY_LIMIT),
            bytes_written: 0,
            bytes_consumed: 0,
        }
    }

    /// Create a new IoBuffer with no capacity limit (use with caution)
    pub fn unlimited() -> Self {
        Self {
            data: VecDeque::new(),
            capacity_limit: None,
            bytes_written: 0,
            bytes_consumed: 0,
        }
    }

    /// Set maximum capacity limit
    pub fn set_capacity_limit(&mut self, limit: Option<usize>) {
        self.capacity_limit = limit;
    }

    /// Get current capacity limit
    pub fn capacity_limit(&self) -> Option<usize> {
        self.capacity_limit
    }

    /// Ensure buffer has at least the specified capacity
    ///
    /// This method ensures the buffer can hold at least `min_capacity` bytes
    /// without reallocation. It's useful for optimizing performance when the
    /// expected data size is known.
    pub fn ensure_capacity(&mut self, min_capacity: usize) -> Result<(), IoBufferError> {
        // Check against capacity limit
        if let Some(limit) = self.capacity_limit {
            if min_capacity > limit {
                return Err(IoBufferError::CapacityExceeded { limit });
            }
        }

        // Reserve additional space if needed
        let current_capacity = self.data.capacity();
        if current_capacity < min_capacity {
            let additional = min_capacity - current_capacity;
            self.data.reserve(additional);
        }

        Ok(())
    }

    /// Try to reserve capacity without failing
    ///
    /// This is the async-friendly version that doesn't allocate immediately
    /// but checks if the allocation would be possible.
    pub fn try_reserve(&self, additional: usize) -> Result<(), IoBufferError> {
        let total_needed = self.len() + additional;

        if let Some(limit) = self.capacity_limit {
            if total_needed > limit {
                return Err(IoBufferError::CapacityExceeded { limit });
            }
        }

        Ok(())
    }

    /// Write data to the buffer (append operation)
    ///
    /// Appends the provided data to the end of the buffer. This is the primary
    /// method for adding data to the buffer.
    pub fn write(&mut self, data: &[u8]) -> Result<usize, IoBufferError> {
        // Check capacity constraints
        self.try_reserve(data.len())?;

        // Append data
        self.data.extend(data);
        self.bytes_written += data.len() as u64;

        Ok(data.len())
    }

    /// Write a single byte to the buffer
    pub fn write_byte(&mut self, byte: u8) -> Result<(), IoBufferError> {
        self.try_reserve(1)?;
        self.data.push_back(byte);
        self.bytes_written += 1;
        Ok(())
    }

    /// Consume and return up to `count` bytes from the front of the buffer
    ///
    /// This method removes and returns data from the front of the buffer.
    /// If fewer than `count` bytes are available, it returns what's available.
    pub fn consume(&mut self, count: usize) -> Vec<u8> {
        let available = self.len();
        let to_consume = count.min(available);

        let mut result = Vec::with_capacity(to_consume);
        for _ in 0..to_consume {
            if let Some(byte) = self.data.pop_front() {
                result.push(byte);
            }
        }

        self.bytes_consumed += result.len() as u64;
        result
    }

    /// Consume exactly `count` bytes or return an error
    ///
    /// This method consumes exactly the requested number of bytes or fails
    /// if insufficient data is available.
    pub fn consume_exact(&mut self, count: usize) -> Result<Vec<u8>, IoBufferError> {
        if self.len() < count {
            return Err(IoBufferError::InsufficientCapacity {
                requested: count,
                available: self.len(),
            });
        }

        Ok(self.consume(count))
    }

    /// Peek at data without consuming it
    ///
    /// Returns a view of up to `count` bytes from the front of the buffer
    /// without removing them.
    pub fn peek(&self, count: usize) -> Vec<u8> {
        let available = self.len().min(count);
        self.data.iter().take(available).copied().collect()
    }

    /// Peek at a specific range of bytes
    ///
    /// Returns bytes from the specified range [start, start+count) without
    /// consuming them. Returns empty vec if range is out of bounds.
    pub fn peek_range(&self, start: usize, count: usize) -> Vec<u8> {
        if start >= self.len() {
            return Vec::new();
        }

        let end = (start + count).min(self.len());
        self.data.range(start..end).copied().collect()
    }

    /// Get the number of bytes currently in the buffer
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get the number of bytes that can be added without reallocation
    pub fn available_capacity(&self) -> usize {
        if let Some(limit) = self.capacity_limit {
            limit.saturating_sub(self.len())
        } else {
            usize::MAX
        }
    }

    /// Clear all data from the buffer
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Shrink the buffer to fit current data
    ///
    /// This operation reduces memory usage by shrinking the internal capacity
    /// to match the current data size.
    pub fn shrink_to_fit(&mut self) {
        self.data.shrink_to_fit();
    }

    /// Get buffer statistics
    pub fn stats(&self) -> IoBufferStats {
        IoBufferStats {
            current_len: self.len(),
            capacity: self.data.capacity(),
            bytes_written: self.bytes_written,
            bytes_consumed: self.bytes_consumed,
            bytes_available: self.available_capacity(),
        }
    }

    /// Append data from another IoBuffer
    ///
    /// This is an efficient operation for combining buffers.
    pub fn append_from(&mut self, other: &mut IoBuffer) -> Result<usize, IoBufferError> {
        let data = other.consume(other.len());
        self.write(&data)
    }

    /// Split the buffer at the specified position
    ///
    /// Returns a new buffer containing data from position onwards,
    /// leaving the original buffer with data before position.
    pub fn split_at(&mut self, pos: usize) -> IoBuffer {
        if pos >= self.len() {
            return IoBuffer::new();
        }

        // Get all data first
        let all_data: Vec<u8> = self.data.iter().copied().collect();

        // Clear current buffer and put first part back
        self.data.clear();
        self.data.extend(&all_data[..pos]);

        // Create new buffer with tail part
        let mut tail_buffer = IoBuffer::with_capacity(all_data.len() - pos);
        tail_buffer
            .write(&all_data[pos..])
            .expect("Write to new buffer should not fail");
        tail_buffer
    }

    /// Find the position of a byte sequence
    ///
    /// Returns the position of the first occurrence of the pattern,
    /// or None if not found. Useful for frame boundary detection.
    pub fn find_pattern(&self, pattern: &[u8]) -> Option<usize> {
        if pattern.is_empty() || pattern.len() > self.len() {
            return None;
        }

        for i in 0..=(self.len() - pattern.len()) {
            let window = self.peek_range(i, pattern.len());
            if window == pattern {
                return Some(i);
            }
        }

        None
    }

    /// Check if buffer starts with the given pattern
    pub fn starts_with(&self, pattern: &[u8]) -> bool {
        if pattern.len() > self.len() {
            return false;
        }

        let prefix = self.peek(pattern.len());
        prefix == pattern
    }
}

impl Default for IoBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about an IoBuffer instance
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoBufferStats {
    /// Current number of bytes in buffer
    pub current_len: usize,
    /// Current allocated capacity
    pub capacity: usize,
    /// Total bytes written since creation
    pub bytes_written: u64,
    /// Total bytes consumed since creation
    pub bytes_consumed: u64,
    /// Bytes that can be added without exceeding limit
    pub bytes_available: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut buffer = IoBuffer::new();

        // Test write and read
        assert_eq!(buffer.write(&[1, 2, 3]).unwrap(), 3);
        assert_eq!(buffer.len(), 3);
        assert!(!buffer.is_empty());

        let data = buffer.consume(2);
        assert_eq!(data, vec![1, 2]);
        assert_eq!(buffer.len(), 1);

        let remaining = buffer.consume(10); // More than available
        assert_eq!(remaining, vec![3]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_peek_operations() {
        let mut buffer = IoBuffer::new();
        buffer.write(&[1, 2, 3, 4, 5]).unwrap();

        // Peek doesn't consume data
        assert_eq!(buffer.peek(3), vec![1, 2, 3]);
        assert_eq!(buffer.len(), 5);

        // Peek range
        assert_eq!(buffer.peek_range(1, 3), vec![2, 3, 4]);
        assert_eq!(buffer.len(), 5);

        // Out of bounds peek
        assert_eq!(buffer.peek_range(10, 5), Vec::<u8>::new());
    }

    #[test]
    fn test_capacity_management() {
        let mut buffer = IoBuffer::with_capacity(10);
        buffer.set_capacity_limit(Some(5));

        // Should succeed
        assert!(buffer.write(&[1, 2, 3]).is_ok());

        // Should fail due to capacity limit
        assert!(buffer.write(&[4, 5, 6]).is_err());
    }

    #[test]
    fn test_pattern_finding() {
        let mut buffer = IoBuffer::new();
        buffer.write(&[1, 2, 3, 4, 2, 3, 5]).unwrap();

        // Find pattern
        assert_eq!(buffer.find_pattern(&[2, 3]), Some(1));
        assert_eq!(buffer.find_pattern(&[2, 3, 4]), Some(1));
        assert_eq!(buffer.find_pattern(&[9, 8]), None);

        // Test starts_with
        assert!(buffer.starts_with(&[1, 2]));
        assert!(!buffer.starts_with(&[2, 3]));
    }

    #[test]
    fn test_statistics() {
        let mut buffer = IoBuffer::new();
        buffer.write(&[1, 2, 3, 4, 5]).unwrap();
        buffer.consume(2);

        let stats = buffer.stats();
        assert_eq!(stats.current_len, 3);
        assert_eq!(stats.bytes_written, 5);
        assert_eq!(stats.bytes_consumed, 2);
    }

    #[test]
    fn test_buffer_splitting() {
        let mut buffer = IoBuffer::new();
        buffer.write(&[1, 2, 3, 4, 5, 6]).unwrap();

        let tail = buffer.split_at(3);
        assert_eq!(buffer.consume(10), vec![1, 2, 3]);
        assert_eq!(tail.peek(10), vec![4, 5, 6]);
    }
}
