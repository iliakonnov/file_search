// See  example: https://github.com/sysnux/btrfs-snapshots-diff/blob/master/btrfs-snapshots-diff.py
//      values: https://github.com/torvalds/linux/blob/master/fs/btrfs/send.h
//      reference: https://github.com/torvalds/linux/blob/master/fs/btrfs/send.c

use crate::model::SubvolumeInfo;
use crate::offseted_reader::OffsetedReader;
use byteorder::{LittleEndian, ReadBytesExt};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

use std::io::{Error, ErrorKind, Read, Result};

#[cfg(feature = "make_dump")]
use super::utils::*;

pub struct Settings {
    pub bypass_errors: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bypass_errors: false,
        }
    }
}

pub struct Parser {
    pub current_subvol: Option<SubvolumeInfo>,
    pub result: Vec<SubvolumeInfo>,
    pub command_no: u64,
    pub default_dt: NaiveDateTime,
    pub settings: Settings,
}

impl Parser {
    pub fn new(settings: Settings) -> Self {
        Self {
            current_subvol: None,
            result: Vec::new(),
            command_no: 0,
            default_dt: NaiveDateTime::new(
                NaiveDate::from_ymd(99999, 12, 31),
                NaiveTime::from_hms(23, 58, 59),
            ),
            settings,
        }
    }

    pub fn parse<T: Read>(mut self, reader: &mut T) -> Result<Vec<SubvolumeInfo>> {
        let mut offseted = OffsetedReader::new(reader);
        Self::read_header(&mut offseted)?;
        loop {
            let res = self.read_command(&mut offseted);
            match res {
                Ok(val) => {
                    if !val {
                        break;
                    }
                }
                Err(err) => {
                    log!("...", &err, &mut offseted, "CMD Err", 0);
                    eprintln!("[{}] CMD Error: {}", offseted.get_offset(), err);
                    // TODO: Log error
                }
            }
        }
        Ok(self.result)
    }

    pub(super) fn subvol(&mut self) -> Result<&mut SubvolumeInfo> {
        self.current_subvol
            .as_mut()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "No subvolume specified"))
    }

    fn read_header<T: Read>(reader: &mut T) -> Result<()> {
        const CORRECT_MAGIC: [u8; 13] = [
            0x62, 0x74, 0x72, 0x66, 0x73, 0x2d, // btrfs-
            0x73, 0x74, 0x72, 0x65, 0x61, 0x6d, // magic
            0x00,
        ];

        let mut magic = [0; 13];
        reader.read_exact(&mut magic)?;

        if magic != CORRECT_MAGIC {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid magic. Found {:?}", magic),
            ));
        }
        let version = reader.read_u32::<LittleEndian>()?;
        if version != 1 {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Invalid version: {}", version),
            ));
        }
        Ok(())
    }
}
