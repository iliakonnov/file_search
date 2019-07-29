use crate::mixed::MixedString;

use crate::offseted_reader::OffsetedReader;
use byteorder::{LittleEndian, ReadBytesExt};
use chrono::NaiveDateTime;

use std::fmt::Debug;
use std::io::{Error, ErrorKind, Read, Result};

use std::io::Cursor;

use super::commands::*;

use super::utils::*;
use crate::btrfs::parser::Parser;

macro_rules! tlv {
    ($wrapper:ident, struct $strct:ident, enum $enm:ident, $reader:ident (
        $( $name:ident : $t:ty = $val:expr, => $convert:ident;)*
    )) => {
        #[derive(Debug)]
        pub enum $wrapper<T> {
            None($enm),
            Some(T)
        }

        impl<T> Into<Option<T>> for $wrapper<T> {
            fn into(self) -> Option<T> {
                match self {
                    $wrapper::None(_) => None,
                    $wrapper::Some(res) => Some(res)
                }
            }
        }

        #[allow(non_snake_case)]
        #[derive(Debug)]
        pub struct $strct {
            $(
                pub $name: $wrapper<$t>
            ),*
        }

        #[derive(Debug)]
        pub enum $enm {
            $(
                $name = $val
            ),*
        }

        impl $enm {
            pub fn new(id: u16) -> Option<Self> {
                match id {
                    $(
                        $val => Some($enm::$name),
                    )*
                    _ => None
                }
            }
        }

        impl $strct {
            pub fn new() -> Self {
                Self {
                    $(
                        $name: $wrapper::None($enm::$name)
                    ),*
                }
            }

            pub fn add<T: Read>(&mut self, id: u16, reader: &mut T) -> Result<()> {
                match id {
                    $(
                        $val => self.$name = $wrapper::Some(reader.$convert::<LittleEndian>()?),
                    )*
                    _ => {}
                }
                Ok(())
            }

            pub fn debug(&self) -> String {
                let mut res = "<TLV ".to_string();
                $(
                    if let $wrapper::Some(val) = &self.$name {
                        res.push_str(&format!("{} = {:?};", stringify!($name), val))
                    }
                )*
                res.push('>');
                res
            }
        }
    };
}

tlv!(TLVValue, struct TLV, enum TLVs, reader (
    UUID: u128 = 1, => read_u128;
    Size: u64 = 4, => read_u64;
    Mode: u64 = 5, => read_u64;
    Uid: u64 = 6, => read_u64;
    Gid: u64 = 7, => read_u64;
    Rdev: u64 = 8, => read_u64;
    Ctime: NaiveDateTime = 9, => read_timespec;
    Mtime: NaiveDateTime = 10, => read_timespec;
    Atime: NaiveDateTime = 11, => read_timespec;
    XattrName: MixedString = 13, => read_mixed;
    XattrData: MixedString = 14, => read_mixed;
    Path: MixedString = 15, => read_mixed;
    PathTo: MixedString = 16, => read_mixed;
    PathLink: MixedString = 17, => read_mixed;
    ClonePath: MixedString = 22, => read_mixed;
));

impl Command {
    fn _tlv_get<T: Debug>(&self, val: TLVValue<T>, def: Option<T>) -> Result<T> {
        match val {
            TLVValue::None(none) => match def {
                Some(val) => {
                    log!(
                        "...",
                        format!("{:?} = {:?} @ {:?}", none, &val, self),
                        &mut OffsetedReader::new(Cursor::new(Vec::new())),
                        "TLV Def",
                        0
                    );
                    Ok(val)
                }
                None => Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("No tlv '{:?}' found in '{:?}'", none, self),
                )),
            },
            TLVValue::Some(res) => Ok(res),
        }
    }

    pub(super) fn tlv_get<T: Debug>(&self, val: TLVValue<T>) -> Result<T> {
        self._tlv_get(val, None)
    }

    pub(super) fn tlv_get_def<T: Debug>(&self, val: TLVValue<T>, def: T) -> Result<T> {
        self._tlv_get(val, Some(def))
    }

    pub(super) fn tlv_get_auto<T: num_traits::Bounded + Debug>(
        &self,
        val: TLVValue<T>,
    ) -> Result<T> {
        self._tlv_get(val, Some(T::max_value()))
    }
}

impl Parser {
    pub(super) fn read_tlvs<T: Read>(&mut self, reader: &mut OffsetedReader<T>) -> Result<TLV> {
        let mut res = TLV::new();
        loop {
            let tlv = try_read(|| reader.read_u16::<LittleEndian>())?;
            let tlv = match tlv {
                Some(val) => val,
                None => break,
            };
            log!(
                hex(&tlv.to_le_bytes()),
                TLVs::new(tlv).map_or("<unknown>".to_string(), |x| format!("{:?}", x)),
                reader,
                "tlv:type",
                2
            );

            let len = reader.read_u16::<LittleEndian>()?;
            log!(hex(&len.to_le_bytes()), len, reader, "tlv:size", 2);

            let mut data = reader.take(len.into());

            #[cfg(feature = "make_dump")]
            let mut data = {
                let data = data.read_bytes::<LittleEndian>()?;
                let unpretty = MixedString::from_bytes(&data).to_string();
                let desc = match len {
                    2 => {
                        let mut arr = [0; 2];
                        arr.clone_from_slice(&data);
                        format!("({}) '{}'", u16::from_le_bytes(arr), unpretty)
                    }
                    4 => {
                        let mut arr = [0; 4];
                        arr.clone_from_slice(&data);
                        format!("({}) '{}'", u32::from_le_bytes(arr), unpretty)
                    }
                    8 => {
                        let mut arr = [0; 8];
                        arr.clone_from_slice(&data);
                        format!("({}) '{}'", u64::from_le_bytes(arr), unpretty)
                    }
                    12 => {
                        let mut a = [0; 8];
                        a.clone_from_slice(&data[..8]);
                        let mut b = [0; 4];
                        b.clone_from_slice(&data[8..]);
                        format!(
                            "({}:{}) '{}'",
                            u64::from_le_bytes(a),
                            u32::from_le_bytes(b),
                            unpretty
                        )
                    }
                    _ => unpretty.to_string(),
                };
                log!(hex(&data), desc, reader, "tlv:data", len as usize);
                Cursor::new(data)
            };

            let added = res.add(tlv, &mut data);
            if let Err(e) = added {
                // TODO: Log error
                log!("...", &e, reader, "TLV Err", 0);
                eprintln!("[{}] TLV Error: {}", reader.get_offset(), e);
                break;
            }
        }
        Ok(res)
    }
}
