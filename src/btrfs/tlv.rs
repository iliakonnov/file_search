use crate::mixed::MixedString;

use crate::offseted_reader::OffsetedReader;
use byteorder::{LittleEndian, ReadBytesExt};
use chrono::NaiveDateTime;

use std::fmt::Debug;
use std::io::{Error, ErrorKind, Read, Result};

#[cfg(feature = "make_dump")]
use std::io::Cursor;

use super::commands::*;

use super::utils::*;
use crate::btrfs::parser::Parser;

macro_rules! tlv {
    ($wrapper:ident, struct $strct:ident, enum $enm:ident, $reader:ident (
        $( $name:ident : $t:ty = $val:expr, => $convert:ident;)*
    )) => {
        #[allow(non_snake_case)]
        #[derive(Debug)]
        pub(super) struct $strct {
            $(
                pub $name: $wrapper<$t>
            ),*
        }

        #[derive(Debug)]
        pub(super) enum $enm {
            $(
                $name = $val
            ),*
        }

        impl $enm {
            pub(super) fn new(id: u16) -> Option<Self> {
                match id {
                    $(
                        $val => Some($enm::$name),
                    )*
                    _ => None
                }
            }
        }

        impl $strct {
            pub(super) fn new() -> Self {
                Self {
                    $(
                        $name: $wrapper::WNone($enm::$name)
                    ),*
                }
            }

            pub(super) fn add<T: Read>(&mut self, id: u16, reader: &mut T) -> Result<()> {
                match id {
                    $(
                        $val => self.$name = $wrapper::WSome(reader.$convert::<LittleEndian>()?),
                    )*
                    _ => {}
                }
                Ok(())
            }

            #[cfg_attr(tarpaulin, skip)]
            pub(super) fn debug(&self) -> String {
                let mut res = "<TLV ".to_string();
                $(
                    if let $wrapper::WSome(val) = &self.$name {
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

#[derive(Debug)]
pub(super) enum TLVValue<T: Debug> {
    WNone(TLVs),
    WSome(T),
}

impl<T: Debug> Into<Option<T>> for TLVValue<T> {
    fn into(self) -> Option<T> {
        match self {
            TLVValue::WNone(_) => None,
            TLVValue::WSome(res) => Some(res),
        }
    }
}

impl<T: Debug> TLVValue<T> {
    pub(super) fn into_option(self) -> Option<T> {
        self.into()
    }
}

impl Command {
    fn _tlv_get<T: Debug>(&self, val: TLVValue<T>, def: Option<T>) -> Result<T> {
        match val {
            TLVValue::WNone(none) => match def {
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
            TLVValue::WSome(res) => Ok(res),
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
            #[cfg_attr(tarpaulin, skip)]
            let mut data = {
                let data = data.read_bytes::<LittleEndian>()?;
                let unpretty = MixedString::from_bytes(&data).to_string();
                let desc = match data.len() {
                    1 => {
                        let mut arr = [0; 1];
                        arr.clone_from_slice(&data);
                        format!("({}) '{}'", u8::from_le_bytes(arr), unpretty)
                    }
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
                    16 => {
                        let mut arr = [0; 16];
                        arr.clone_from_slice(&data);
                        format!("({}) '{}'", u128::from_le_bytes(arr), unpretty)
                    }
                    _ => unpretty.to_string(),
                };
                log!(hex(&data), desc, reader, "tlv:data", len as usize);
                Cursor::new(data)
            };

            let added = res.add(tlv, &mut data);
            if let Err(e) = added {
                log!("...", &e, reader, "TLV Err", 0);
                eprintln!("[{}] TLV Error: {}", reader.get_offset(), e);
                break;
            }
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::Parser;
    use crate::btrfs::commands::Command;
    use crate::btrfs::parser::Settings;
    use crate::btrfs::tlv::{TLVValue, TLVs, TLV};
    use crate::offseted_reader::OffsetedReader;
    use std::io::Cursor;

    const DATA: &[u8] = &[
        0x05, 0x00, // type: 5 = "Mode"
        0x08, 0x00, // length: 8 (u64)
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // DATA_MODE
    ];
    const DATA_MODE: u64 = 0x08_07_06_05_04_03_02_01;
    const DATA_INVALID: &[u8] = &[
        0x05, 0x00, // type: 5 = "Mode"
        0x08, 0x00, // length: 8 (u64)
              // No data (truncated)
    ];
    const DATA_MIXED: &[u8] = &[
        0x05, 0x00, // type: 5 = "Mode"
        0x08, 0x00, // length: 8 (u64)
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // DATA_MODE
        0x07, 0x00, // type: 7 = "Gid"
        0x02, 0x00, // length: 2 (must be 8)
        0x00, 0x00, // Some data
    ];

    const ZERO: [u8; 8] = [0; 8];
    const ONE: [u8; 8] = [0x01, 0, 0, 0, 0, 0, 0, 0];
    const TWO: [u8; 8] = [0x02, 0, 0, 0, 0, 0, 0, 0];

    const GID: u16 = 7;
    const MODE: u16 = 5;

    #[test]
    fn tlv_value_into_none() {
        let val = TLVValue::WNone(TLVs::Mode);
        let res: Option<()> = val.into();
        assert!(res.is_none())
    }

    #[test]
    fn tlv_value_into_some() {
        let val = TLVValue::WSome(123);
        let res: Option<u8> = val.into();
        assert!(res.is_some());
        assert_eq!(res.unwrap(), 123);
    }

    #[test]
    fn default_none() {
        let tlv = TLV::new();

        let t: Option<u64> = tlv.Mode.into();
        assert!(t.is_none());

        let t: Option<u64> = tlv.Gid.into();
        assert!(t.is_none());
    }

    #[test]
    fn fill_tlv() {
        let mut tlv = TLV::new();

        let res = tlv.add(MODE, &mut Cursor::new(&ONE));
        assert!(res.is_ok());

        let res = tlv.add(GID, &mut Cursor::new(&TWO));
        assert!(res.is_ok());

        assert_eq!(tlv.Mode.into_option().unwrap_or(0), 1);
        assert_eq!(tlv.Gid.into_option().unwrap_or(0), 2);
    }

    #[test]
    fn get_auto() {
        let val: TLVValue<u64> = TLVValue::WNone(TLVs::Mode);
        let cmd = Command::Unknown;

        let res = cmd.tlv_get_auto(val);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), u64::max_value());
    }

    #[test]
    fn get_def() {
        let val: TLVValue<u64> = TLVValue::WNone(TLVs::Mode);
        let cmd = Command::Unknown;

        let res = cmd.tlv_get_def(val, 123);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 123);
    }

    #[test]
    fn get_some() {
        let val: TLVValue<u64> = TLVValue::WSome(123);
        let cmd = Command::Unknown;

        let res = cmd.tlv_get(val);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 123);
    }

    #[test]
    fn get_none() {
        let val: TLVValue<u64> = TLVValue::WNone(TLVs::Mode);
        let cmd = Command::Unknown;

        let res = cmd.tlv_get(val);
        assert!(res.is_err());
    }

    #[test]
    fn read_data() {
        let data = DATA.to_vec();
        let mut reader = OffsetedReader::new(Cursor::new(data));

        let mut parser = Parser::new(Settings::default());
        let tlvs = parser.read_tlvs(&mut reader);

        assert!(tlvs.is_ok());
        let tlvs = tlvs.unwrap();

        let cmd = Command::Unknown;
        let res = cmd.tlv_get(tlvs.Mode);

        assert!(res.is_ok());
        assert_eq!(res.unwrap(), DATA_MODE)
    }

    #[test]
    fn read_error() {
        let data = DATA_INVALID.to_vec();
        let mut reader = OffsetedReader::new(Cursor::new(data));

        let mut parser = Parser::new(Settings::default());
        let tlvs = parser.read_tlvs(&mut reader);

        assert!(tlvs.is_ok());
        let tlvs = tlvs.unwrap();

        let cmd = Command::Unknown;
        let res = cmd.tlv_get(tlvs.Mode);

        assert!(res.is_err());
    }

    #[test]
    fn read_mixed() {
        let data = DATA_MIXED.to_vec();
        let mut reader = OffsetedReader::new(Cursor::new(data));

        let mut parser = Parser::new(Settings::default());
        let tlvs = parser.read_tlvs(&mut reader);

        assert!(tlvs.is_ok());
        let tlvs = tlvs.unwrap();

        let cmd = Command::Unknown;
        let res = cmd.tlv_get(tlvs.Mode);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), DATA_MODE);

        let res = cmd.tlv_get(tlvs.Gid);
        assert!(res.is_err());
    }

    #[test]
    #[cfg(feature = "make_dump")]
    fn dump() {
        let datasets: &[&[u8]] = &[
            &[
                0x05, 0x00, 1, 0x00, // 1 byte
                0,
            ],
            &[
                0x05, 0x00, 2, 0x00, // 2 byte
                0, 0,
            ],
            &[
                0x05, 0x00, 4, 0x00, // 4 byte
                0, 0, 0, 0,
            ],
            &[
                0x05, 0x00, 8, 0x00, // 8 byte
                0, 0, 0, 0, 0, 0, 0, 0,
            ],
            &[
                0x09, 0x00, 12, 0x00, // 12 byte
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            &[
                0x01, 0x00, 16, 0x00, // 16 byte
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        ];
        for data in datasets {
            let mut reader = OffsetedReader::new(Cursor::new(data));

            let mut parser = Parser::new(Settings::default());
            let tlvs = parser.read_tlvs(&mut reader);

            assert!(tlvs.is_ok());
        }
    }
}
