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

    pub(super) fn del_file(&mut self, path: MixedString) -> Result<()> {
        match self.files.entry(path) {
            Entry::Vacant(entry) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Deleting, but file not found: {}", entry.key()),
                ));
            }
            Entry::Occupied(mut entry) => match entry.get() {
                Some(_) => {
                    if self.overwrite {
                        entry.remove();
                    } else {
                        entry.insert(None);
                    }
                }
                None => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Deleting, but file already deleted: {}", entry.key()),
                    ))
                }
            },
        }
        Ok(())
    }

    pub(super) fn get_file(&mut self, path: &MixedString) -> Result<&mut Option<FileInfo>> {
        self.load_file(path);
        self.files.get_mut(path).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Accessing, but old file not found: {}", path),
            )
        })
    }

    pub(super) fn pop_file(&mut self, path: &MixedString) -> Result<Option<FileInfo>> {
        self.load_file(path);
        self.files.remove(path).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Renaming, but old file not found: {}", path),
            )
        })
    }

    /// Loads file from database to subvolume. Useful for modifying files, but useless if in overwrite mode
    pub(super) fn load_file(&mut self, path: &MixedString) {
        if self.overwrite {
            return;
        }
        if self.files.contains_key(path) {
            return;
        }
        // TODO: Not implemented
    }

    pub(super) fn copy_file(&mut self, from: &MixedString, to: MixedString) -> Result<()> {
        self.load_file(from);
        let mut entry = self.get_file(from)?.clone();
        if let Some(info) = &mut entry {
            info.filename = to.clone();
        }
        self.files.insert(to, entry);
        Ok(())
    }

    pub(super) fn modify<T, F>(&mut self, path: MixedString, f: Debuggable<F>) -> Result<T>
    where
        F: FnOnce(&mut FileInfo) -> T,
    {
        self.load_file(&path);
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

#[cfg(test)]
mod tests {
    use crate::btrfs::utils::Debuggable;
    use crate::mixed::MixedString;
    use crate::model::{FileInfo, FileType, SubvolumeInfo, SubvolumeSource};
    use std::collections::HashMap;

    fn get_subvol(overwrite: bool) -> SubvolumeInfo {
        SubvolumeInfo {
            source: SubvolumeSource::Btrfs { uuid: 0 },
            overwrite,
            files: HashMap::new(),
        }
    }

    fn validate(info: &FileInfo, path: &MixedString) {
        assert_eq!(&info.filename, path);
        assert_eq!(info.permissions, 123);
        assert_eq!(info.filetype, FileType::Unknown);
    }

    #[test]
    fn add() {
        let mut info = get_subvol(true);
        let path: MixedString = "a/b/c".into();
        info.add_file(path.clone(), FileType::Unknown, 123).unwrap();

        let v = info.files.get(&path).unwrap();
        let v = v.as_ref().unwrap();
        validate(v, &path);
    }

    #[test]
    fn pop() {
        let mut info = get_subvol(true);
        let path: MixedString = "a/b/c".into();
        info.add_file(path.clone(), FileType::Unknown, 123).unwrap();

        let pop = info.pop_file(&path).unwrap();
        let pop = pop.unwrap();

        validate(&pop, &path);
    }

    #[test]
    fn pop_unexisting() {
        let mut info = get_subvol(false);
        let path: MixedString = "a/b/c".into();

        let res = info.pop_file(&path);
        assert!(res.is_err())
    }

    #[test]
    fn get() {
        let mut info = get_subvol(true);
        let path: MixedString = "a/b/c".into();
        info.add_file(path.clone(), FileType::Unknown, 123).unwrap();

        let get = info.get_file(&path).unwrap();
        let get = get.as_ref().unwrap();

        validate(get, &path);
    }

    #[test]
    fn get_unexisting() {
        let mut info = get_subvol(false);
        let path: MixedString = "a/b/c".into();

        let res = info.get_file(&path);
        assert!(res.is_err())
    }

    #[test]
    fn copy() {
        let mut info = get_subvol(true);
        let path: MixedString = "a/b/c".into();
        info.add_file(path.clone(), FileType::Unknown, 123).unwrap();

        let new_path: MixedString = "d/e/f".into();
        info.copy_file(&path, new_path.clone()).unwrap();

        let new = info.files.get(&new_path).unwrap().as_ref().unwrap();
        validate(new, &new_path);

        let old = info.files.get(&path).unwrap().as_ref().unwrap();
        validate(old, &path);
    }

    #[test]
    fn modify() {
        let mut info = get_subvol(true);
        let path: MixedString = "a/b/c".into();
        info.add_file(path.clone(), FileType::Unknown, 123).unwrap();

        info.modify(
            path.clone(),
            debuggable!(|x: &mut FileInfo| x.user_id = 999),
        )
        .unwrap();

        let v = info.files.get(&path).unwrap().as_ref().unwrap();
        validate(v, &path);
        assert_eq!(v.user_id, 999);
    }

    #[test]
    fn modify_err_unexisting() {
        let mut info = get_subvol(false);
        let path: MixedString = "a/b/c".into();
        let res = info.modify(path, debuggable!(|x: &mut FileInfo| x.user_id = 999));

        assert!(res.is_err());
    }

    #[test]
    fn modify_err_deleted() {
        let mut info = get_subvol(false);
        let path: MixedString = "a/b/c".into();

        info.add_file(path.clone(), FileType::Unknown, 123).unwrap();
        info.del_file(path.clone()).unwrap();

        let res = info.modify(path, debuggable!(|x: &mut FileInfo| x.user_id = 999));

        assert!(res.is_err());
    }

    #[test]
    fn del() {
        let mut info = get_subvol(true);
        let path: MixedString = "a/b/c".into();
        info.add_file(path.clone(), FileType::Unknown, 123).unwrap();

        info.del_file(path.clone()).unwrap();
        assert!(!info.files.contains_key(&path));
    }

    #[test]
    fn del_twice() {
        let mut info = get_subvol(false);
        let path: MixedString = "a/b/c".into();
        info.add_file(path.clone(), FileType::Unknown, 123).unwrap();

        info.del_file(path.clone()).unwrap();
        let res = info.del_file(path);
        assert!(res.is_err());
    }

    #[test]
    fn del_unexisting() {
        let mut info = get_subvol(false);
        let path: MixedString = "a/b/c".into();

        let res = info.del_file(path);
        assert!(res.is_err());
    }

    #[test]
    fn del_no_overwrite() {
        let mut info = get_subvol(false);
        let path: MixedString = "a/b/c".into();
        info.add_file(path.clone(), FileType::Unknown, 123).unwrap();

        info.del_file(path.clone()).unwrap();
        let f = info.files.get(&path).unwrap();
        assert!(f.is_none());
    }
}
