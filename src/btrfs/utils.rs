use crate::mixed::MixedString;

use byteorder::{ByteOrder, ReadBytesExt};
use chrono::NaiveDateTime;

use std::fmt;
use std::io::{Error, ErrorKind, Read, Result};

#[cfg(feature = "make_dump")]
use crate::offseted_reader::OffsetedReader;
use num_traits::FromPrimitive;
#[cfg(feature = "make_dump")]
use std::convert::TryInto;
#[cfg(feature = "make_dump")]
use std::fmt::{Debug, Display};

// https://users.rust-lang.org/t/is-it-possible-to-implement-debug-for-fn-type/14824/3
pub(super) struct Debuggable<T: ?Sized> {
    pub text: &'static str,
    pub value: T,
}

impl<T: ?Sized> fmt::Debug for Debuggable<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl<T: ?Sized> ::std::ops::Deref for Debuggable<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

macro_rules! debuggable {
    ($($body:tt)+) => {
        Debuggable {
            text: stringify!($($body)+),
            value: $($body)+,
        }
    };
}

#[cfg(feature = "make_dump")]
pub(super) fn hex(arr: &[u8]) -> String {
    if arr.len() > 15 {
        let mut res = String::new();
        for b in &arr[..4] {
            res.push_str(&format!("{:02x} ", b));
        }
        res.push_str("... ");
        for (i, b) in arr[arr.len() - 4..].iter().enumerate() {
            let x = if i == 3 {
                format!("{:02x}", b)
            } else {
                format!("{:02x} ", b)
            };
            res.push_str(&x);
        }
        res
    } else if arr.len() > 10 {
        let mut res = String::new();
        for b in arr {
            res.push_str(&format!("{:02x}", b));
        }
        res
    } else {
        let mut res = String::new();
        for (i, b) in arr.iter().enumerate() {
            let x = if i == arr.len() - 1 {
                format!("{:02x}", b)
            } else {
                format!("{:02x} ", b)
            };
            res.push_str(&x);
        }
        res
    }
}

#[cfg(feature = "make_dump")]
#[cfg_attr(tarpaulin, skip)]
pub(super) fn _log<D: Display, T: Debug, R: Read>(
    hex: D,
    val: T,
    reader: &mut OffsetedReader<R>,
    description: &str,
    len: usize,
) {
    let current: isize = reader.get_offset().try_into().unwrap_or(isize::max_value());
    let len: isize = len.try_into().unwrap_or(isize::max_value());
    let begin = if len == 0 { 0 } else { current - len };

    println!(
        "[{: <10}] {: <30} [{: <10}] {: <10} {:?}",
        begin,
        hex,
        current - 1,
        description,
        val
    );
}

pub(super) fn nop<T: std::any::Any>(_x: T) {}

macro_rules! log {
    ($($args:tt)*) => {
        #[cfg(feature="make_dump")]
        #[cfg_attr(tarpaulin, skip)]
        {
            _log($($args)*)
        }
    };
}

pub(super) trait AdvancedReader {
    fn read_timespec<T: ByteOrder>(&mut self) -> Result<NaiveDateTime>;
    fn read_mixed<T: ByteOrder>(&mut self) -> Result<MixedString>;
    fn read_bytes<T: ByteOrder>(&mut self) -> Result<Vec<u8>>;
}

impl<U: Read> AdvancedReader for U {
    fn read_timespec<T: ByteOrder>(&mut self) -> Result<NaiveDateTime> {
        let s = self.read_u64::<T>()?;
        let ns = self.read_u32::<T>()?;
        match i64::from_u64(s) {
            None => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Too much seconds: {}", s),
            )),
            Some(s) => NaiveDateTime::from_timestamp_opt(s, ns).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Invalid timestamp: {}s {}ns", s, ns),
                )
            }),
        }
    }

    fn read_mixed<T: ByteOrder>(&mut self) -> Result<MixedString> {
        let bytes = self.read_bytes::<T>()?;
        let s = MixedString::from_bytes(&bytes);
        Ok(s)
    }

    fn read_bytes<T: ByteOrder>(&mut self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

pub(super) fn try_read<T, F: FnOnce() -> Result<T>>(r: F) -> Result<Option<T>> {
    let res = r();
    match res {
        Err(err) => match err.kind() {
            ErrorKind::UnexpectedEof => Ok(None),
            _ => Err(err),
        },
        Ok(val) => Ok(Some(val)),
    }
}

#[cfg(test)]
mod tests {
    use crate::btrfs::utils::AdvancedReader;
    use crate::mixed::MixedString;
    use byteorder::BigEndian;
    use std::io::Cursor;

    #[cfg(feature = "make_dump")]
    mod hex_tests {
        use crate::btrfs::utils::hex;

        #[test]
        fn hex_small() {
            let data = &[0, 1, 2];
            let hexed = hex(data);
            assert_eq!(hexed, "00 01 02");
        }

        #[test]
        fn hex_medium() {
            let data = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
            let hexed = hex(data);
            assert_eq!(hexed, "000102030405060708090a0b0c0d");
        }

        #[test]
        fn hex_large() {
            let data = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
            let hexed = hex(data);
            assert_eq!(hexed, "00 01 02 03 ... 0d 0e 0f 10");
        }
    }

    #[test]
    fn read_timespec() {
        const DAYS: u64 = 9012;
        const SECONDS: u64 = 1234;
        const NANOS: u32 = 5678;

        const SECS: u64 = (DAYS * 24 * 3600) + SECONDS;
        #[allow(clippy::cast_possible_wrap)]
        const EXPECTED: i64 = (SECS * 1_000_000_000 + NANOS as u64) as i64;

        let mut data = Vec::new();
        data.extend_from_slice(&SECS.to_be_bytes());
        data.extend_from_slice(&NANOS.to_be_bytes());

        let mut reader = Cursor::new(data);
        let ts = reader.read_timespec::<BigEndian>();
        let ts = ts.unwrap();

        assert_eq!(ts.timestamp_nanos(), EXPECTED);
    }

    #[test]
    fn timespec_too_much_seconds() {
        const SECS: u64 = u64::max_value();
        const NANOS: u32 = 5678;

        let mut data = Vec::new();
        data.extend_from_slice(&SECS.to_be_bytes());
        data.extend_from_slice(&NANOS.to_be_bytes());

        let mut reader = Cursor::new(data);
        let ts = reader.read_timespec::<BigEndian>();
        assert!(ts.is_err())
    }

    #[test]
    fn timespec_invalid_nanos() {
        const SECS: u64 = 10;
        const NANOS: u32 = 2_000_000_000;

        let mut data = Vec::new();
        data.extend_from_slice(&SECS.to_be_bytes());
        data.extend_from_slice(&NANOS.to_be_bytes());

        let mut reader = Cursor::new(data);
        let ts = reader.read_timespec::<BigEndian>();
        assert!(ts.is_err())
    }

    #[test]
    fn timespec_invalid_secs() {
        const SECS: u64 = i64::max_value() as u64;
        const NANOS: u32 = 0;

        let mut data = Vec::new();
        data.extend_from_slice(&SECS.to_be_bytes());
        data.extend_from_slice(&NANOS.to_be_bytes());

        let mut reader = Cursor::new(data);
        let ts = reader.read_timespec::<BigEndian>();
        assert!(ts.is_err())
    }

    #[test]
    fn read_bytes() {
        let data: [u8; 3] = [1, 2, 3];

        let mut reader = Cursor::new(&data);
        let bytes = reader.read_bytes::<BigEndian>();
        let bytes = bytes.unwrap();

        assert_eq!(bytes, data);
    }

    #[test]
    fn read_bytes_empty() {
        let data: [u8; 0] = [];

        let mut reader = Cursor::new(&data);
        let bytes = reader.read_bytes::<BigEndian>();
        let bytes = bytes.unwrap();

        assert_eq!(bytes, data);
    }

    #[test]
    fn read_mixed() {
        let expected = MixedString::from_string("Hello!".to_string());
        let data = expected.to_bytes();

        let mut reader = Cursor::new(&data);
        let s = reader.read_mixed::<BigEndian>();
        let s = s.unwrap();

        assert_eq!(s, expected);
    }
}
