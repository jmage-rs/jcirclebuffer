A circle buffer for use with std::io::Read/

```
use jcirclebuffer::CircleBuffer;
use std::io::Read;
let mut some_read = std::io::Cursor::new(b"banana");

let mut my_buf = CircleBuffer::default();
let read_zone: &mut [u8] = my_buf.get_fillable_area().unwrap();
let read_amount = Read::read(&mut some_read, read_zone).unwrap();

assert!(my_buf.view_nocopy().is_empty());
my_buf.fill(read_amount);
assert_eq!(my_buf.view_nocopy(), b"banana");
my_buf.consume(2);
assert_eq!(my_buf.view_nocopy(), b"nana");
```

The buffer is implemented as single unmoving memory buffer that keeps track of the "start"
point and occupied length. [CircleBuffer::get_fillable_area] will return the current
_contiguous_ fillable area. Depending on the location of the "wrap point" (the end of the
underlying buffer) it may be appropriate to fill the entire fillable area, then immediately
request a new fillable area without consuming any data.
The example below shows how the circle buffer handles wrapping.

```
use jcirclebuffer::CircleBuffer;
use std::io::Read;
let mut some_read = std::io::Cursor::new(b"abc");
let mut other_read = std::io::Cursor::new(b"defghijk");
let mut my_buf = CircleBuffer::with_size(4);

let read_zone: &mut [u8] = my_buf.get_fillable_area().unwrap();
let read_amount = Read::read(&mut some_read, read_zone).unwrap();
my_buf.fill(read_amount);

assert_eq!(read_amount, 3);
assert_eq!(my_buf.view_nocopy(), b"abc");
my_buf.consume(2);
assert_eq!(my_buf.view_nocopy(), b"c");
assert_eq!(my_buf.get_fillable_area().unwrap().len(), 1);

let read_zone: &mut [u8] = my_buf.get_fillable_area().unwrap();
let read_amount = Read::read(&mut other_read, read_zone).unwrap();
assert_eq!(read_amount, 1);
my_buf.fill(read_amount);
assert_eq!(my_buf.get_fillable_area().unwrap(), b"ab");
```

If you want to view a contiguous version of the possibly discontiguous data in the buffer,
you can use [CircleBuffer::view]. This will show contiguous data in-place but will perform
a copy if the desired data crosses the "wrap point"

```
use jcirclebuffer::CircleBuffer;
use std::io::Read;
let mut some_read = std::io::Cursor::new(b"abcdefghijk");
let mut my_buf = CircleBuffer::with_size(4);

let read_zone = my_buf.get_fillable_area().unwrap();
let read_amount = Read::read(&mut some_read, read_zone).unwrap();
my_buf.fill(read_amount);
my_buf.consume(1);
let read_zone = my_buf.get_fillable_area().unwrap();
let read_amount = Read::read(&mut some_read, read_zone).unwrap();
my_buf.fill(read_amount);

// Underlying memory layout is b"ebcd"
assert_eq!(my_buf.view_parts(4), (&b"bcd"[..], &b"e"[..]));
my_buf.view(4, |data| assert_eq!(data, b"bcde")); // requires feature "std"

```

You can keep a circle buffer entirely on the stack using [CircleBuffer::new]:

```
use jcirclebuffer::CircleBuffer;
CircleBuffer::new([0; 4]); // Does not require feature "std"
```
