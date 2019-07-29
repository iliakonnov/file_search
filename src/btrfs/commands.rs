use crate::model::{FileInfo, FileType, SubvolumeInfo, SubvolumeSource};
use crate::offseted_reader::OffsetedReader;
use byteorder::{LittleEndian, ReadBytesExt};

use std::collections::HashMap;

use std::io::{Error, ErrorKind, Read, Result};

use crate::btrfs::utils::Debuggable;

use super::parser::*;
use super::utils::*;

macro_rules! cmd {
    (enum $strct:ident {
        $($name:ident = $val:expr,)*
    }) => {
        #[derive(Debug)]
        pub enum $strct {
            $(
                $name = $val,
            )*
            Unknown
        }

        impl $strct {
            pub fn new(id: u16) -> Self {
                match id {
                    $(
                        $val => $strct::$name,
                    )*
                    _ => $strct::Unknown
                }
            }
        }
    };
}

cmd!(
    enum Command {
        Subvolume = 1,
        Snapshot = 2, // diff
        MkFile = 3,
        MkDir = 4,
        MkNod = 5,
        MkFIFO = 6,
        MkSock = 7,
        Symlink = 8,
        Rename = 9,
        Link = 10,
        Unlink = 11,
        Rmdir = 12,
        SetXattr = 13,
        RemoveXattr = 14,
        Clone = 16,
        Chmod = 18,
        Chown = 19,
        Utimes = 20,
        End = 21,
    }
);

impl Parser {
    pub(super) fn read_command<T: Read>(&mut self, reader: &mut OffsetedReader<T>) -> Result<bool> {
        let size = try_read(|| reader.read_u32::<LittleEndian>())?;
        let size = match size {
            None => return Ok(false),
            Some(val) => val,
        };
        log!(hex(&size.to_le_bytes()), size, reader, "cmd:size", 4);

        let cmd_id = reader.read_u16::<LittleEndian>()?;
        let cmd = Command::new(cmd_id);
        log!(hex(&cmd_id.to_le_bytes()), &cmd, reader, "cmd:cmd", 2);

        let checksum = reader.read_u32::<LittleEndian>()?;
        log!(hex(&checksum.to_le_bytes()), checksum, reader, "cmd:crc", 4);
        // TODO: Check CRC32
        nop(checksum);

        let mut tlvs = OffsetedReader::after(reader.get_offset(), reader.take(size.into()));
        let tlv = self.read_tlvs(&mut tlvs)?;
        log!("...", tlv.debug(), reader, "cmd:tlvs", 0);

        self.command_no += 1;

        match cmd {
            Command::Unknown => {}
            Command::Subvolume => {
                if self.current_subvol.is_some() {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "New subvolume while previous still parsing",
                    ));
                }
                self.current_subvol = Some(SubvolumeInfo {
                    source: SubvolumeSource::Btrfs {
                        uuid: cmd.tlv_get_auto(tlv.UUID)?,
                    },
                    overwrite: true,
                    files: HashMap::new(),
                });
            }
            Command::Snapshot => {
                if self.current_subvol.is_some() {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "New snapshot while previous still parsing",
                    ));
                }
                self.current_subvol = Some(SubvolumeInfo {
                    source: SubvolumeSource::Btrfs {
                        uuid: cmd.tlv_get_auto(tlv.UUID)?,
                    },
                    overwrite: false,
                    files: HashMap::new(),
                });
            }
            Command::MkFile | Command::MkDir => {
                let path = cmd.tlv_get(tlv.Path)?;
                self.subvol()?.add_file(path, FileType::Directory, 0)?;
            }
            Command::MkNod | Command::MkSock | Command::MkFIFO => {
                let path = cmd.tlv_get(tlv.Path)?;
                let mode = cmd.tlv_get_auto(tlv.Mode)?;
                let _rdev = cmd.tlv_get_auto(tlv.Rdev)?;
                self.subvol()?.add_file(path, FileType::Directory, mode)?;
            }
            Command::Symlink => {
                let path = cmd.tlv_get(tlv.Path)?;
                let _from = cmd.tlv_get(tlv.PathLink)?;
                self.subvol()?.add_file(path, FileType::Symlink, 0)?;
            }
            Command::Rename => {
                let from = cmd.tlv_get(tlv.Path)?;
                let to = cmd.tlv_get(tlv.PathTo)?;
                let subvol = self.subvol()?;

                subvol.load_file(&from);

                let entry = subvol.pop_file(&from)?;
                subvol.files.insert(to, entry);
            }
            Command::Link => {
                self.subvol()?
                    .copy_file(&cmd.tlv_get(tlv.Path)?, cmd.tlv_get(tlv.PathLink)?)?;
            }
            Command::Unlink | Command::Rmdir => {
                let path = cmd.tlv_get(tlv.Path)?;
                let subvol = self.subvol()?;
                subvol.load_file(&path);
                subvol.files.remove(&path).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidData, "Deleting file that does not exists")
                })?;
            }
            Command::SetXattr | Command::RemoveXattr => {
                // TODO
                if false {
                    unimplemented!()
                }
            }
            Command::Clone => {
                self.subvol()?
                    .copy_file(&cmd.tlv_get(tlv.Path)?, cmd.tlv_get(tlv.ClonePath)?)?;
            }
            Command::Chmod => {
                let path = cmd.tlv_get(tlv.Path)?;
                let mode = cmd.tlv_get_auto(tlv.Mode)?;

                self.subvol()?.modify(
                    path,
                    debuggable!(|info: &mut FileInfo| {
                        info.permissions = mode;
                    }),
                )?;
            }
            Command::Chown => {
                let path = cmd.tlv_get(tlv.Path)?;
                let user = cmd.tlv_get_auto(tlv.Uid)?;
                let group = cmd.tlv_get_auto(tlv.Gid)?;

                self.subvol()?.modify(
                    path,
                    debuggable!(|info: &mut FileInfo| {
                        info.user_id = user;
                        info.group_id = group;
                    }),
                )?;
            }
            Command::Utimes => {
                let path = cmd.tlv_get(tlv.Path)?;
                let accessed = cmd.tlv_get_def(tlv.Atime, self.default_dt)?;
                let created = cmd.tlv_get_def(tlv.Ctime, self.default_dt)?;
                let modified = cmd.tlv_get_def(tlv.Mtime, self.default_dt)?;
                self.subvol()?.modify(
                    path,
                    debuggable!(|info: &mut FileInfo| {
                        info.accessed = accessed;
                        info.created = created;
                        info.modified = modified;
                    }),
                )?;
            }
            Command::End => {
                let subvol = std::mem::replace(&mut self.current_subvol, None);
                let subvol = subvol.ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "End command, but no subvolume started",
                    )
                })?;
                self.result.push(subvol);
            }
        }

        Ok(true)
    }
}
