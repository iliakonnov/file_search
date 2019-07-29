use crate::mixed::MixedString;
use crate::model::{FileInfo, SubvolumeInfo, SubvolumeSource};
use chrono::NaiveDateTime;
use std::collections::HashMap;
use std::io;
use std::os::linux::fs::MetadataExt;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;

use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

trait IntoNaive {
    fn into_naive(self) -> NaiveDateTime;
}

impl IntoNaive for SystemTime {
    fn into_naive(self) -> NaiveDateTime {
        let (sec, nanos) = match self.duration_since(UNIX_EPOCH) {
            Ok(dur) => {
                let (sec, nanos) = (dur.as_secs(), dur.subsec_nanos());
                #[allow(clippy::cast_possible_wrap)]
                let sec = sec as i64;
                (sec, nanos)
            }
            Err(e) => {
                // unlikely but should be handled
                let dur = e.duration();
                let (sec, nanos) = (dur.as_secs(), dur.subsec_nanos());
                #[allow(clippy::cast_possible_wrap)]
                let sec = sec as i64;
                if nanos == 0 {
                    (-sec, 0)
                } else {
                    (-sec - 1, 1_000_000_000 - nanos)
                }
            }
        };
        NaiveDateTime::from_timestamp(sec, nanos)
    }
}

//noinspection RsUnresolvedReference
pub fn walk(path: MixedString) -> io::Result<SubvolumeInfo> {
    let walker = WalkDir::new(path.to_string());
    let mut result = HashMap::new();
    for res in walker {
        if let Ok(entry) = res {
            let path = entry.path().as_os_str().as_bytes();
            let path = MixedString::from_bytes(path);

            let meta = std::fs::File::open(entry.path())?.metadata()?;

            let info = FileInfo {
                filename: path.clone(),
                permissions: meta.permissions().mode().into(),
                modified: meta.modified()?.into_naive(),
                accessed: meta.accessed()?.into_naive(),
                created: meta.created()?.into_naive(),
                length: meta.len(),
                user_id: meta.st_uid().into(),
                group_id: meta.st_gid().into(),
                filetype: entry.file_type().into(),
            };
            result.insert(path, Some(info));
        }
    }
    Ok(SubvolumeInfo {
        source: SubvolumeSource::Find { path },
        overwrite: true,
        files: result,
    })
}
