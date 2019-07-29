// See  example: https://github.com/sysnux/btrfs-snapshots-diff/blob/master/btrfs-snapshots-diff.py
//      values: https://github.com/torvalds/linux/blob/master/fs/btrfs/send.h
//      reference: https://github.com/torvalds/linux/blob/master/fs/btrfs/send.c

use crate::mixed::MixedString;
use crate::model::{FileInfo, FileType, SubvolumeInfo, SubvolumeSource};
use crate::offseted_reader::OffsetedReader;
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt;
use std::fmt::Debug;
use std::io::{Cursor, Error, ErrorKind, Read, Result};

#[cfg(feature = "make_dump")]
use std::fmt::Display;

// https://users.rust-lang.org/t/is-it-possible-to-implement-debug-for-fn-type/14824/3
pub struct Debuggable<T: ?Sized> {
    text: &'static str,
    value: T,
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
fn hex(arr: &[u8]) -> String {
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
fn _log<D: Display, T: Debug, R: Read>(
    hex: D,
    val: T,
    reader: &mut OffsetedReader<R>,
    description: &str,
    len: usize,
) {
    let current = reader.get_offset();
    let begin = if len != 0 { current - len } else { 0 };
    println!(
        "[{: <10}] {: <30} [{: <10}] {: <10} {:?}",
        begin,
        hex,
        current - 1,
        description,
        val
    );
}

macro_rules! log {
    ($($args:tt)*) => {
        #[cfg(feature="make_dump")]
        {
            _log($($args)*)
        }
    };
}

trait AdvancedReader {
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

fn try_read<T, F: FnOnce() -> Result<T>>(r: F) -> Result<Option<T>> {
    let res = r();
    match res {
        Err(err) => match err.kind() {
            ErrorKind::UnexpectedEof => Ok(None),
            _ => Err(err),
        },
        Ok(val) => Ok(Some(val)),
    }
}

macro_rules! tlv {
    ($wrapper:ident, struct $strct:ident, enum $enm:ident, $reader:ident (
        $( $name:ident : $t:ty = $val:expr, => $convert:ident;)*
    )) => {
        #[derive(Debug)]
        enum $wrapper<T> {
            None($enm),
            Some(T)
        }

        impl<T> Into<Option<T>> for $wrapper<T> {
            fn into(self) -> Option<T> {
                match self {
                    Self::None(_) => None,
                    Self::Some(res) => Some(res)
                }
            }
        }

        #[allow(non_snake_case)]
        #[derive(Debug)]
        struct $strct {
            $(
                $name: $wrapper<$t>
            ),*
        }

        #[derive(Debug)]
        enum $enm {
            $(
                $name = $val
            ),*
        }

        impl $enm {
            fn new(id: u16) -> Option<Self> {
                match id {
                    $(
                        $val => Some(Self::$name),
                    )*
                    _ => None
                }
            }
        }

        impl $strct {
            fn new() -> Self {
                Self {
                    $(
                        $name: $wrapper::None($enm::$name)
                    ),*
                }
            }

            fn add<T: Read>(&mut self, id: u16, reader: &mut T) -> Result<()> {
                match id {
                    $(
                        $val => self.$name = $wrapper::Some(reader.$convert::<LittleEndian>()?),
                    )*
                    _ => {}
                }
                Ok(())
            }

            fn debug(&self) -> String {
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

fn _tlv_get<T: Debug>(cmd: &Command, val: TLVValue<T>, def: Option<T>) -> Result<T> {
    match val {
        TLVValue::None(none) => match def {
            Some(val) => {
                let _r = &mut OffsetedReader::new(Cursor::new(Vec::new()));
                log!(
                    "...",
                    format!("{:?} = {:?} @ {:?}", none, &val, cmd),
                    r,
                    "TLV Def",
                    0
                );
                Ok(val)
            }
            None => Err(Error::new(
                ErrorKind::InvalidData,
                format!("No tlv '{:?}' found in '{:?}'", none, cmd),
            )),
        },
        TLVValue::Some(res) => Ok(res),
    }
}

fn tlv_get<T: Debug>(cmd: &Command, val: TLVValue<T>) -> Result<T> {
    _tlv_get(cmd, val, None)
}

fn tlv_get_def<T: Debug>(cmd: &Command, val: TLVValue<T>, def: T) -> Result<T> {
    _tlv_get(cmd, val, Some(def))
}

fn tlv_get_auto<T: num_traits::Bounded + Debug>(cmd: &Command, val: TLVValue<T>) -> Result<T> {
    _tlv_get(cmd, val, Some(T::max_value()))
}

macro_rules! cmd {
    (enum $strct:ident {
        $($name:ident = $val:expr,)*
    }) => {
        #[derive(Debug)]
        enum $strct {
            $(
                $name = $val,
            )*
            Unknown
        }

        impl $strct {
            fn new(id: u16) -> Self {
                match id {
                    $(
                        $val => Self::$name,
                    )*
                    _ => Self::Unknown
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

struct Header {
    magic: String,
    version: u32,
}

trait SubvolumeExt {
    fn add_file(&mut self, path: MixedString, filetype: FileType, mode: u64) -> Result<()>;
    fn get_file(&mut self, path: &MixedString) -> Result<&mut Option<FileInfo>>;
    fn pop_file(&mut self, path: &MixedString) -> Result<Option<FileInfo>>;
    fn load_file(&mut self, path: &MixedString);
    fn copy_file(&mut self, from: MixedString, to: MixedString) -> Result<()>;
    fn modify<T, F>(&mut self, path: MixedString, f: Debuggable<F>) -> Result<T>
    where
        F: FnOnce(&mut FileInfo) -> T;
}

impl SubvolumeExt for SubvolumeInfo {
    fn add_file(&mut self, path: MixedString, filetype: FileType, mode: u64) -> Result<()> {
        self.files.insert(
            path.clone(),
            Some(FileInfo {
                filename: path,
                permissions: mode,
                modified: NaiveDateTime::from_timestamp(0, 0),
                accessed: NaiveDateTime::from_timestamp(0, 0),
                created: NaiveDateTime::from_timestamp(0, 0),
                length: 0,
                user_id: 0,
                group_id: 0,
                filetype,
            }),
        );
        Ok(())
    }

    fn get_file(&mut self, path: &MixedString) -> Result<&mut Option<FileInfo>> {
        self.files.get_mut(path).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Renaming, but old file not found: {}", path),
            )
        })
    }

    fn pop_file(&mut self, path: &MixedString) -> Result<Option<FileInfo>> {
        self.files.remove(path).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Renaming, but old file not found: {}", path),
            )
        })
    }

    fn load_file(&mut self, path: &MixedString) {
        if self.overwrite {
            return;
        }
        if self.files.contains_key(path) {
            return;
        }
        unimplemented!()
    }

    fn copy_file(&mut self, from: MixedString, to: MixedString) -> Result<()> {
        self.load_file(&from);
        let entry = self.get_file(&from)?.clone();
        self.files.insert(to, entry);
        Ok(())
    }

    fn modify<T, F>(&mut self, path: MixedString, f: Debuggable<F>) -> Result<T>
    where
        F: FnOnce(&mut FileInfo) -> T,
    {
        match self.files.entry(path) {
            Entry::Occupied(mut val) => match val.get_mut() {
                Some(info) => {
                    let func = f.value;
                    Ok(func(info))
                }
                None => Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Modify deleted file: {}. `{:?}`", val.key(), f),
                )),
            },
            Entry::Vacant(vac) => Err(Error::new(
                ErrorKind::InvalidData,
                format!("Modify file that does not exists: {}. `{:?}`", vac.key(), f),
            )),
        }
    }
}

pub struct Settings {
    pub bypass_errors: bool,
}

pub struct Parser {
    current_subvol: Option<SubvolumeInfo>,
    result: Vec<SubvolumeInfo>,
    command_no: u64,
    default_dt: NaiveDateTime,
    settings: Settings,
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

    fn subvol(&mut self) -> Result<&mut SubvolumeInfo> {
        self.current_subvol
            .as_mut()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "No subvolume specified"))
    }

    fn read_command<T: Read>(&mut self, reader: &mut OffsetedReader<T>) -> Result<bool> {
        let size = try_read(|| reader.read_u32::<LittleEndian>())?;
        let size = match size {
            None => return Ok(false),
            Some(val) => val,
        };
        log!(hex(&size.to_le_bytes()), size, reader, "cmd:size", 4);

        let cmd_id = reader.read_u16::<LittleEndian>()?;
        let cmd = Command::new(cmd_id);
        log!(hex(&cmd_id.to_le_bytes()), &cmd, reader, "cmd:cmd", 2);

        let _checksum = reader.read_u32::<LittleEndian>()?;
        log!(hex(&checksum.to_le_bytes()), checksum, reader, "cmd:crc", 4);
        // TODO: validate checksum

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
                        uuid: tlv_get_auto(&cmd, tlv.UUID)?,
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
                        uuid: tlv_get_auto(&cmd, tlv.UUID)?,
                    },
                    overwrite: false,
                    files: HashMap::new(),
                });
            }
            Command::MkFile | Command::MkDir => {
                let path = tlv_get(&cmd, tlv.Path)?;
                self.subvol()?.add_file(path, FileType::Directory, 0)?;
            }
            Command::MkNod | Command::MkSock | Command::MkFIFO => {
                let path = tlv_get(&cmd, tlv.Path)?;
                let mode = tlv_get_auto(&cmd, tlv.Mode)?;
                let _rdev = tlv_get_auto(&cmd, tlv.Rdev)?;
                self.subvol()?.add_file(path, FileType::Directory, mode)?;
            }
            Command::Symlink => {
                let path = tlv_get(&cmd, tlv.Path)?;
                let _from = tlv_get(&cmd, tlv.PathLink)?;
                self.subvol()?.add_file(path, FileType::Symlink, 0)?;
            }
            Command::Rename => {
                let from = tlv_get(&cmd, tlv.Path)?;
                let to = tlv_get(&cmd, tlv.PathTo)?;
                let subvol = self.subvol()?;

                subvol.load_file(&from);

                let entry = subvol.pop_file(&from)?;
                subvol.files.insert(to, entry);
            }
            Command::Link => {
                self.subvol()?
                    .copy_file(tlv_get(&cmd, tlv.Path)?, tlv_get(&cmd, tlv.PathLink)?)?;
            }
            Command::Unlink | Command::Rmdir => {
                let path = tlv_get(&cmd, tlv.Path)?;
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
                    .copy_file(tlv_get(&cmd, tlv.Path)?, tlv_get(&cmd, tlv.ClonePath)?)?;
            }
            Command::Chmod => {
                let path = tlv_get(&cmd, tlv.Path)?;
                let mode = tlv_get_auto(&cmd, tlv.Mode)?;

                self.subvol()?.modify(
                    path,
                    debuggable!(|info: &mut FileInfo| {
                        info.permissions = mode;
                    }),
                )?;
            }
            Command::Chown => {
                let path = tlv_get(&cmd, tlv.Path)?;
                let user = tlv_get_auto(&cmd, tlv.Uid)?;
                let group = tlv_get_auto(&cmd, tlv.Gid)?;

                self.subvol()?.modify(
                    path,
                    debuggable!(|info: &mut FileInfo| {
                        info.user_id = user;
                        info.group_id = group;
                    }),
                )?;
            }
            Command::Utimes => {
                let path = tlv_get(&cmd, tlv.Path)?;
                let accessed = tlv_get_def(&cmd, tlv.Atime, self.default_dt)?;
                let created = tlv_get_def(&cmd, tlv.Ctime, self.default_dt)?;
                let modified = tlv_get_def(&cmd, tlv.Mtime, self.default_dt)?;
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

    fn read_tlvs<T: Read>(&mut self, reader: &mut OffsetedReader<T>) -> Result<TLV> {
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
