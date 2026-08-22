//! A fixed-capacity byte buffer for streaming frame decode.
//!
//! `mbus-rs`'s `util::IoBuffer` cannot move here: it is built on `VecDeque`, and its API
//! is explicitly growable (`unlimited()`, `set_capacity_limit`), which has no meaning
//! without an allocator. It also has other users that legitimately want that behaviour.
//!
//! [`FrameDecoder`](super::frame_decode::FrameDecoder) needs five operations, so this
//! provides exactly those over a fixed array rather than porting 437 lines of buffer that
//! would have to lose half its API on the way.

/// Bytes the decode buffer holds.
///
/// The largest wM-Bus frame on air is 288 bytes — `L(255)` plus the L byte plus a CRC
/// after block 0 and every 16-byte block thereafter (see
/// [`packet_size`](super::framing::packet_size)). 512 leaves room for a complete frame
/// plus the leading part of the next one, which is what lets a caller feed the radio's
/// FIFO in whatever chunk sizes it happens to deliver.
pub const DECODE_BUFFER_CAPACITY: usize = 512;

/// The buffer is full and cannot accept more bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferFull;

impl core::fmt::Display for BufferFull {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "decode buffer full")
    }
}

impl core::error::Error for BufferFull {}

/// Observability for the decode buffer. Counters only — the caller decides what to do
/// with them, and nothing here logs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DecodeBufferStats {
    pub current_len: usize,
    pub capacity: usize,
    pub bytes_written: u64,
    pub bytes_consumed: u64,
}

/// A fixed-capacity FIFO of bytes awaiting decode.
#[derive(Debug)]
pub struct DecodeBuffer {
    buf: heapless::Deque<u8, DECODE_BUFFER_CAPACITY>,
    bytes_written: u64,
    bytes_consumed: u64,
}

impl DecodeBuffer {
    pub const fn new() -> Self {
        Self {
            buf: heapless::Deque::new(),
            bytes_written: 0,
            bytes_consumed: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Append bytes. Fails rather than dropping them silently: a caller that overruns
    /// the buffer has a real problem — usually a stream that never yields a valid
    /// header — and hiding it would turn a stall into mystery data loss.
    pub fn write(&mut self, data: &[u8]) -> Result<(), BufferFull> {
        if data.len() > DECODE_BUFFER_CAPACITY - self.buf.len() {
            return Err(BufferFull);
        }
        for &b in data {
            // Space was checked above, so this cannot fail.
            let _ = self.buf.push_back(b);
        }
        self.bytes_written += data.len() as u64;
        Ok(())
    }

    /// Copy the first `count` bytes without consuming them. Returns fewer if the buffer
    /// holds fewer.
    pub fn peek(&self, count: usize, out: &mut [u8]) -> usize {
        let n = count.min(self.buf.len()).min(out.len());
        for (slot, byte) in out.iter_mut().zip(self.buf.iter()).take(n) {
            *slot = *byte;
        }
        n
    }

    /// Remove and return exactly `count` bytes, or fail if fewer are held.
    pub fn consume_exact<const N: usize>(
        &mut self,
        count: usize,
    ) -> Result<heapless::Vec<u8, N>, BufferFull> {
        if count > self.buf.len() || count > N {
            return Err(BufferFull);
        }
        let mut out = heapless::Vec::new();
        for _ in 0..count {
            if let Some(b) = self.buf.pop_front() {
                let _ = out.push(b);
            }
        }
        self.bytes_consumed += count as u64;
        Ok(out)
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }

    pub fn stats(&self) -> DecodeBufferStats {
        DecodeBufferStats {
            current_len: self.buf.len(),
            capacity: DECODE_BUFFER_CAPACITY,
            bytes_written: self.bytes_written,
            bytes_consumed: self.bytes_consumed,
        }
    }
}

impl Default for DecodeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_peek_consume_round_trip() {
        let mut b = DecodeBuffer::new();
        b.write(&[1, 2, 3, 4]).unwrap();
        assert_eq!(b.len(), 4);

        let mut head = [0u8; 2];
        assert_eq!(b.peek(2, &mut head), 2);
        assert_eq!(head, [1, 2]);
        assert_eq!(b.len(), 4, "peek must not consume");

        let taken: heapless::Vec<u8, 8> = b.consume_exact(3).unwrap();
        assert_eq!(&taken[..], &[1, 2, 3]);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn peek_returns_what_it_has_not_what_was_asked() {
        let mut b = DecodeBuffer::new();
        b.write(&[9]).unwrap();
        let mut out = [0u8; 4];
        assert_eq!(b.peek(4, &mut out), 1, "only one byte is available");
    }

    #[test]
    fn consume_more_than_held_fails_rather_than_truncating() {
        let mut b = DecodeBuffer::new();
        b.write(&[1, 2]).unwrap();
        assert!(b.consume_exact::<8>(3).is_err());
        assert_eq!(b.len(), 2, "a failed consume must not disturb the buffer");
    }

    #[test]
    fn overrun_is_reported_not_silently_dropped() {
        let mut b = DecodeBuffer::new();
        let big = [0u8; DECODE_BUFFER_CAPACITY];
        b.write(&big).unwrap();
        assert_eq!(b.write(&[1]), Err(BufferFull));
        assert_eq!(
            b.len(),
            DECODE_BUFFER_CAPACITY,
            "rejected write changes nothing"
        );
    }

    #[test]
    fn stats_track_totals_across_clear() {
        let mut b = DecodeBuffer::new();
        b.write(&[1, 2, 3]).unwrap();
        let _: heapless::Vec<u8, 4> = b.consume_exact(2).unwrap();
        b.clear();
        let s = b.stats();
        assert_eq!(s.bytes_written, 3);
        assert_eq!(s.bytes_consumed, 2);
        assert_eq!(s.current_len, 0);
        assert_eq!(s.capacity, DECODE_BUFFER_CAPACITY);
    }
}
