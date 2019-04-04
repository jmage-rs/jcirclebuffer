#![cfg_attr(not(feature = "std"), no_std)]

//! A circle buffer for use with [std::io::Read]
//! ```
//! # use jcirclebuffer::CircleBuffer;
//! use std::io::Read;
//! let mut some_read = std::io::Cursor::new(b"banana");
//!
//! let mut my_buf = CircleBuffer::default();
//! let read_zone: &mut [u8] = my_buf.get_fillable_area().unwrap();
//! let read_amount = Read::read(&mut some_read, read_zone).unwrap();
//!
//! assert!(my_buf.view_nocopy().is_empty());
//! my_buf.fill(read_amount);
//! assert_eq!(my_buf.view_nocopy(), b"banana");
//! my_buf.consume(2);
//! assert_eq!(my_buf.view_nocopy(), b"nana");
//! ```
//! The buffer is implemented as single unmoving memory buffer that keeps track of the "start"
//! point and occupied length. [CircleBuffer::get_fillable_area] will return the current
//! _contiguous_ fillable area. Depending on the location of the "wrap point" (the end of the
//! underlying buffer) it may be appropriate to fill the entire fillable area, then immediately
//! request a new fillable area without consuming any data.
//!
//! The example below shows how the circle buffer handles wrapping.
//! ```
//! # use jcirclebuffer::CircleBuffer;
//! # use std::io::Read;
//! let mut some_read = std::io::Cursor::new(b"abc");
//! let mut other_read = std::io::Cursor::new(b"defghijk");
//! let mut my_buf = CircleBuffer::with_size(4);
//!
//! let read_zone: &mut [u8] = my_buf.get_fillable_area().unwrap();
//! let read_amount = Read::read(&mut some_read, read_zone).unwrap();
//! my_buf.fill(read_amount);
//!
//! assert_eq!(read_amount, 3);
//! assert_eq!(my_buf.view_nocopy(), b"abc");
//! my_buf.consume(2);
//! assert_eq!(my_buf.view_nocopy(), b"c");
//! assert_eq!(my_buf.get_fillable_area().unwrap().len(), 1);
//!
//! let read_zone: &mut [u8] = my_buf.get_fillable_area().unwrap();
//! let read_amount = Read::read(&mut other_read, read_zone).unwrap();
//! assert_eq!(read_amount, 1);
//! my_buf.fill(read_amount);
//! assert_eq!(my_buf.get_fillable_area().unwrap(), b"ab");
//! ```
//! If you want to view a contiguous version of the possibly discontiguous data in the buffer,
//! you can use [CircleBuffer::view]. This will show contiguous data in-place but will perform
//! a copy if the desired data crosses the "wrap point"
//! ```
//! # use jcirclebuffer::CircleBuffer;
//! # use std::io::Read;
//! let mut some_read = std::io::Cursor::new(b"abcdefghijk");
//! let mut my_buf = CircleBuffer::with_size(4);
//!
//! let read_zone = my_buf.get_fillable_area().unwrap();
//! let read_amount = Read::read(&mut some_read, read_zone).unwrap();
//! my_buf.fill(read_amount);
//! my_buf.consume(1);
//! let read_zone = my_buf.get_fillable_area().unwrap();
//! let read_amount = Read::read(&mut some_read, read_zone).unwrap();
//! my_buf.fill(read_amount);
//!
//! // Underlying memory layout is b"ebcd"
//! assert_eq!(my_buf.view_parts(4), (&b"bcd"[..], &b"e"[..]));
//! my_buf.view(4, |data| assert_eq!(data, b"bcde")); // requires feature "std"
//! ```
//! You can keep a circle buffer entirely on the stack using [CircleBuffer::new]:
//! ```
//! # use jcirclebuffer::CircleBuffer;
//! CircleBuffer::new([0; 4]); // Does not require feature "std"
//! ```

/// A circle buffer based on an unmoving underlying buffer.
pub struct CircleBuffer<T> {
    start: usize,
    len: usize,
    buf: T,
}

#[cfg(feature = "std")]
impl Default for CircleBuffer<Vec<u8>> {
    /// An easy way to get a heap allocated circle buffer. Backed by a 1MiB [std::vec::Vec]. Requires "std".
    fn default() -> Self {
        CircleBuffer::with_size(1_048_576) // 1MiB
    }
}

#[cfg(feature = "std")]
impl CircleBuffer<Vec<u8>> {
    /// Request a heap allocated circle buffer of a certain size. Requires "std".
    pub fn with_size(size: usize) -> Self {
        let buf = vec![0; size];
        CircleBuffer {
            start: 0,
            len: 0,
            buf,
        }
    }
}

impl<T> CircleBuffer<T>
where
    T: AsRef<[u8]> + AsMut<[u8]>,
{
    /// Make a circle buffer backed by a user-provided buffer. This can be used to make a stack allocated circle buffer.
    pub fn new(buf: T) -> CircleBuffer<T> {
        CircleBuffer {
            start: 0,
            len: 0,
            buf,
        }
    }

    /// Request the size of the underlying buffer. Doesn't change for the life of the circle buffer.
    pub fn size(&self) -> usize {
        self.buf.as_ref().len()
    }

    /// Indicate that a certain amount of the buffer has been filled with meaningful content.
    /// Almost always used as:
    /// ```
    /// # use jcirclebuffer::CircleBuffer;
    /// # let mut my_buf = CircleBuffer::default();
    /// # let mut something = std::io::Cursor::new(b"banana");
    /// let read_zone = my_buf.get_fillable_area().unwrap();
    /// let read_amount = std::io::Read::read(&mut something, read_zone).unwrap();
    /// my_buf.fill(read_amount);
    /// ```
    pub fn fill(&mut self, amt: usize) {
        self.len = self.len.checked_add(amt).unwrap();
        assert!(self.len <= self.size());
    }

    #[cfg(feature = "std")]
    /// A convenience wrapper around get_fillable_area() -> Read::read() -> buf.fill(amt).
    /// Doesn't fill() if Read::read returns an error.
    pub fn read<U>(&mut self, reader: &mut U) -> std::io::Result<usize>
    where
        U: std::io::Read,
    {
        let read_zone = self.get_fillable_area().expect("read buffer full");
        let result = std::io::Read::read(reader, read_zone);
        if let Ok(amt) = result {
            self.fill(amt);
        }
        result
    }

    /// The current amount of meaningful data in the buffer. fill() makes this go up, consume() makes it go down.
    pub fn len(&self) -> usize {
        self.len
    }

    /// len() == 0
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// len() == size()
    pub fn is_full(&self) -> bool {
        self.len == self.size()
    }

    #[cfg(feature = "std")]
    /// Allows a contiguous view of potentially non-contiguous underlying data. MAY INCUR A COPY. Should only incur copies rarely if the size of the buffer is large relative to the possible message size.
    pub fn view<R>(&self, amt: usize, callback: impl FnOnce(&[u8]) -> R) -> R {
        let (head, tail) = self.view_parts(amt);
        if tail.is_empty() {
            return callback(head);
        }
        let mut view_buffer = vec![0; head.len() + tail.len()];
        view_buffer[..head.len()].copy_from_slice(head);
        view_buffer[head.len()..].copy_from_slice(tail);
        callback(&view_buffer)
    }

    /// View potentially non-contiguous data. Will never incur a copy. Returns (head, tail). All the data will be in the head unless data crosses the wrap point.
    pub fn view_parts(&self, amt: usize) -> (&[u8], &[u8]) {
        assert!(amt <= self.len);
        let start = self.start;
        let view_end = start.checked_add(amt).unwrap();
        if view_end <= self.size() {
            return (&self.buf.as_ref()[start..view_end], &[]);
        }
        let buf = self.buf.as_ref();
        let (left, data_head) = buf.split_at(start);
        let (data_tail, _) = left.split_at(view_end % self.size());
        return (data_head, data_tail);
    }

    /// Returns the maximum amount of meaningful contiguous data. Will never incur a copy.
    pub fn view_nocopy(&self) -> &[u8] {
        let mut view_end = self.start.checked_add(self.len).unwrap();
        if view_end > self.size() {
            view_end = self.size();
        }
        &self.buf.as_ref()[self.start..view_end]
    }

    /// Marks data as consumed. Advances the "start" cursor by amt. If this results in the buffer being empty, moves the start cursor to 0. Does not touch the underlying buffer.
    pub fn consume(&mut self, amt: usize) {
        self.len = self.len.checked_sub(amt).unwrap();
        if self.len == 0 {
            self.start = 0;
        } else {
            self.start = self.start.checked_add(amt).unwrap() % self.size();
        }
    }

    /// Returns the next contiguous unused area in the underlying buffer. Returns None if the buffer is full.
    /// There are potentially two separate contiguous unused areas in the buffer at any one time. If you use up one of them (and call fill()) then you will be able to get to the other one.
    pub fn get_fillable_area(&mut self) -> Option<&mut [u8]> {
        if self.len == self.size() {
            return None;
        }

        let start = self.start;
        let end = self.start.checked_add(self.len).unwrap() % self.size();
        if end < start {
            Some(&mut self.buf.as_mut()[end..start])
        } else {
            Some(&mut self.buf.as_mut()[end..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CircleBuffer;
    #[test]
    fn circle_buffer_tests() {
        let mut circle_buffer = CircleBuffer::default();
        circle_buffer.fill(5);
        assert!(circle_buffer.view_nocopy().len() == 5);
    }
}
