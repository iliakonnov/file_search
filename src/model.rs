use crate::mixed::MixedString;
use chrono::NaiveDateTime;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::FileTypeExt;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum FileType {
    File = 0,
    Directory = 1,
    Symlink = 2,

    BlockDevice = 3,
    CharDevice = 4,
    Fifo = 5,
    Socket = 6,

    Unknown,
}

impl FileType {
    pub const fn to_num(self) -> u8 {
        self as u8
    }
}

impl From<fs::FileType> for FileType {
    fn from(t: fs::FileType) -> Self {
        if t.is_dir() {
            FileType::Directory
        } else if t.is_file() {
            FileType::File
        } else if t.is_symlink() {
            FileType::Symlink
        } else if t.is_block_device() {
            FileType::BlockDevice
        } else if t.is_char_device() {
            FileType::CharDevice
        } else if t.is_fifo() {
            FileType::Fifo
        } else if t.is_socket() {
            FileType::Socket
        } else {
            FileType::Unknown
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileInfo {
    pub filename: MixedString,
    // https://doc.rust-lang.org/std/os/unix/fs/trait.PermissionsExt.html#tymethod.mode
    pub permissions: u64,
    pub modified: NaiveDateTime,
    pub accessed: NaiveDateTime,
    pub created: NaiveDateTime,
    pub length: u64,
    pub user_id: u64,
    pub group_id: u64,
    pub filetype: FileType,
}

#[derive(Debug)]
pub enum SubvolumeSource {
    Btrfs { uuid: u128 },
    Find { path: MixedString },
}

#[derive(Debug)]
pub struct SubvolumeInfo {
    pub source: SubvolumeSource,
    pub overwrite: bool,
    pub files: HashMap<MixedString, Option<FileInfo>>,
}
