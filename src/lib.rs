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

    /// Copy data into the circle buffer, possibly crossing the wrap point. Does fill() automatically. Panics if capacity is not available.
    pub fn extend(&mut self, data: &[u8]) {
        let head = self.get_fillable_area().unwrap();
        let head_amt = core::cmp::min(data.len(), head.len());
        head[..head_amt].copy_from_slice(&data[..head_amt]);
        self.fill(head_amt);
        if head_amt == data.len() {
            return;
        }
        let tail = self.get_fillable_area().unwrap();
        let remainder = data.len().checked_sub(head_amt).unwrap();
        tail[..remainder].copy_from_slice(&data[head_amt..]);
        self.fill(remainder);
    }

    /// The current amount of meaningful data in the buffer. fill() makes this go up, consume() makes it go down.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Discard all information in the buffer and realign at the start point. This is a cheap operation that makes no modification to the underlying buffer.
    pub fn clear(&mut self) {
        self.len = 0;
        self.start = 0;
    }

    /// The total amount of free space available for filling.
    pub fn available(&self) -> usize {
        self.size() - self.len()
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
    /// Allows a contiguous view of potentially non-contiguous underlying data. MAY INCUR A COPY. Should only incur copies rarely if the size of the buffer is large relative to the possible message size. Requires feature "std".
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

    /// Allows a contgious view of potentially non-contiguous data using a user-provided buffer. May incur a copy but will not incur a heap allocation. Available without feature "std".
    pub fn view_provided<C, R>(&self, buf: &mut [u8], callback: C) -> R
    where
        C: FnOnce(&[u8]) -> R,
    {
        let amt = buf.len();
        let (head, tail) = self.view_parts(amt);
        if tail.is_empty() {
            return callback(head);
        }
        buf[..head.len()].copy_from_slice(head);
        buf[head.len()..].copy_from_slice(tail);
        callback(buf)
    }

    /// view_provided but mut. Changes made to the view slice will be reflected in the only the circlebuffer buffer the view did not cross a wrap point and will be reflected in only in the provided buffer if the view did cross a wrap point.
    pub fn view_provided_mut<C, R>(&mut self, buf: &mut [u8], callback: C) -> R
    where
        C: FnOnce(&mut [u8]) -> R,
    {
        let amt = buf.len();
        let (head, tail) = self.view_parts_mut(amt);
        if tail.is_empty() {
            return callback(head);
        }
        buf[..head.len()].copy_from_slice(head);
        buf[head.len()..].copy_from_slice(tail);
        callback(buf)
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

    /// view_parts but mutable.
    pub fn view_parts_mut(&mut self, amt: usize) -> (&mut [u8], &mut [u8]) {
        assert!(amt <= self.len);
        let start = self.start;
        let view_end = start.checked_add(amt).unwrap();
        if view_end <= self.size() {
            return (&mut self.buf.as_mut()[start..view_end], &mut []);
        }
        let remainder: usize = view_end % self.size();
        let buf = self.buf.as_mut();
        let (left, data_head) = buf.split_at_mut(start);
        let (data_tail, _) = left.split_at_mut(remainder);
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

impl<T> std::io::Write for CircleBuffer<T>
where
    T: AsRef<[u8]> + AsMut<[u8]>,
{
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let available = self.available();
        if available == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Full"));
        }
        let amt = std::cmp::min(data.len(), available);
        self.extend(&data[..amt]);
        Ok(amt)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<T> std::io::Read for CircleBuffer<T>
where
    T: AsRef<[u8]> + AsMut<[u8]>,
{
    fn read(&mut self, dest: &mut [u8]) -> std::io::Result<usize> {
        let amt = std::cmp::min(self.len(), dest.len());
        let parts = self.view_parts(amt);
        dest[..parts.0.len()].copy_from_slice(parts.0);
        dest[parts.0.len()..amt].copy_from_slice(parts.1);
        self.consume(amt);
        Ok(amt)
    }
}

#[cfg(test)]
mod tests {
    use super::CircleBuffer;
    #[test]
    fn circle_buffer_tests() {
        let mut read = std::io::Cursor::new(b"abcdefghijklmnopqrstuvwxyz");
        let mut circle_buffer = CircleBuffer::new([0u8; 4]);
        assert!(circle_buffer.is_empty());
        let read_size = circle_buffer.read(&mut read).unwrap();
        assert_eq!(read_size, 4);
        assert_eq!(circle_buffer.len(), read_size);
        assert_eq!(circle_buffer.view_nocopy(), b"abcd");
        assert_eq!(circle_buffer.view_parts(4), (&b"abcd"[..], &b""[..]));
        assert!(circle_buffer.is_full());
        assert!(circle_buffer.get_fillable_area().is_none());
        circle_buffer.view(4, |data| assert_eq!(data, b"abcd"));
        let mut view_buf = [0u8; 4];
        circle_buffer.view_provided(&mut view_buf, |data| assert_eq!(data, b"abcd"));

        circle_buffer.consume(2);
        assert_eq!(circle_buffer.view_nocopy(), b"cd");
        let read_size = circle_buffer.read(&mut read).unwrap();
        assert_eq!(read_size, 2);
        assert_eq!(circle_buffer.view_parts(4), (&b"cd"[..], &b"ef"[..]));
        circle_buffer.view(4, |data| assert_eq!(data, b"cdef"));
        let mut view_buf = [0u8; 4];
        circle_buffer.view_provided(&mut view_buf[..], |data| assert_eq!(data, b"cdef"));
        assert_eq!(circle_buffer.view_nocopy(), b"cd");
        circle_buffer.consume(4);

        let mut big_buffer = CircleBuffer::default();
        assert_eq!(big_buffer.read(&mut read).unwrap(), 20);

        big_buffer.extend(b"banana");
        big_buffer.consume(20);
        big_buffer.view(6, |x| assert_eq!(x, b"banana"));

        let mut buffer = CircleBuffer::new([0u8; 4]);
        buffer.extend(b"abcd");
        buffer.consume(2);
        buffer.extend(b"ef");
        let mut read_buf = [0u8; 3];
        let result = std::io::Read::read(&mut buffer, &mut read_buf).unwrap();
        assert_eq!(result, 3);
        assert_eq!(b"cde", &read_buf);
        assert_eq!(buffer.len(), 1);
    }
}
