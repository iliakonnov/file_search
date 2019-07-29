use crate::mixed::MixedString;

use byteorder::{ByteOrder, ReadBytesExt};
use chrono::NaiveDateTime;

use std::fmt;
use std::io::{Error, ErrorKind, Read, Result};

#[cfg(feature = "make_dump")]
use crate::offseted_reader::OffsetedReader;
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
        res.push_str(" ... ");
        for (i, b) in arr[arr.len() - 4..].iter().enumerate() {
            let x = if i == arr.len() - 1 {
                format!("{:02x}", b)
            } else {
                format!("{:02x} ", b)
            };
            res.push_str(&x);
        }
        res
    } else if arr.len() > 10 {
        let mut res = String::new();
        for (i, b) in arr.iter().enumerate() {
            let mut x = format!("{:02x}", b);
            if i & 1 == 0 {
                x = x.to_uppercase();
            }
            res.push_str(&x);
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
    let current = reader.get_offset() as isize;
    let len = len as isize;
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
        #[allow(clippy::cast_possible_wrap)]
        let s = s as i64;
        let ns = self.read_u32::<T>()?;
        NaiveDateTime::from_timestamp_opt(s, ns).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid timestamp: {}s {}ns", s, ns),
            )
        })
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
