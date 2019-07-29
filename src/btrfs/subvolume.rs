use crate::mixed::MixedString;
use crate::model::{FileInfo, FileType, SubvolumeInfo};

use chrono::NaiveDateTime;
use std::collections::hash_map::Entry;

use std::io::{Error, ErrorKind, Result};

use crate::btrfs::utils::Debuggable;

impl SubvolumeInfo {
    pub(super) fn add_file(
        &mut self,
        path: MixedString,
        filetype: FileType,
        mode: u64,
    ) -> Result<()> {
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

    pub(super) fn get_file(&mut self, path: &MixedString) -> Result<&mut Option<FileInfo>> {
        self.files.get_mut(path).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Renaming, but old file not found: {}", path),
            )
        })
    }

    pub(super) fn pop_file(&mut self, path: &MixedString) -> Result<Option<FileInfo>> {
        self.files.remove(path).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Renaming, but old file not found: {}", path),
            )
        })
    }

    pub(super) fn load_file(&mut self, path: &MixedString) {
        if self.overwrite {
            return;
        }
        if self.files.contains_key(path) {
            return;
        }
        unimplemented!()
    }

    pub(super) fn copy_file(&mut self, from: &MixedString, to: MixedString) -> Result<()> {
        self.load_file(from);
        let entry = self.get_file(from)?.clone();
        self.files.insert(to, entry);
        Ok(())
    }

    pub(super) fn modify<T, F>(&mut self, path: MixedString, f: Debuggable<F>) -> Result<T>
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
