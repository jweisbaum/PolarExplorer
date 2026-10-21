//! A Hindsight Sync archive on this computer: the same keys and byte ranges
//! the S3 transport asks for, read from files under one folder.
use bytes::Bytes;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::s3::{Object, Range};
use crate::{EnvError, Result};

/// The most one read may return, as for S3.
const LIMIT: u64 = 16 << 20;

#[derive(Debug)]
pub(super) struct FileStore {
    root: PathBuf,
}

impl FileStore {
    /// # Errors
    /// `root` is not a readable directory; the message names it.
    pub fn new(root: &Path) -> Result<Self> {
        std::fs::read_dir(root).map_err(|e| {
            EnvError::Open(format!(
                "the archive folder {} cannot be read: {e}",
                root.display()
            ))
        })?;
        Ok(Self {
            root: root.to_owned(),
        })
    }

    /// `None` when the file does not exist (missing data, like a 404 shard).
    pub async fn get(
        &self,
        key: &str,
        range: Option<Range>,
        cancel: &AtomicBool,
    ) -> Result<Option<Object>> {
        // Only our fixed metadata and numerical chunk keys can be requested.
        if key.contains("..")
            || key.starts_with('/')
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_./".contains(&c))
        {
            return Err(EnvError::Layout("invalid Whirlwind object key".into()));
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(EnvError::Cancelled);
        }
        let (root, key) = (self.root.clone(), key.to_owned());
        tokio::task::spawn_blocking(move || read(&root, &key, range))
            .await
            .map_err(|_| EnvError::Open("Whirlwind file worker failed".into()))?
    }
}

fn read(root: &Path, key: &str, range: Option<Range>) -> Result<Option<Object>> {
    let path = root.join(key);
    let fail = |e: std::io::Error| EnvError::Open(format!("{}: {e}", path.display()));
    let mut file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // A folder deleted under the reader is an error, not missing data.
            if !root.is_dir() {
                return Err(EnvError::Open(format!(
                    "the archive folder {} is gone",
                    root.display()
                )));
            }
            return Ok(None);
        }
        Err(e) => return Err(fail(e)),
    };
    if !file.metadata().map_err(fail)?.is_file() {
        return Ok(None);
    }
    let size = file.metadata().map_err(fail)?.len();
    let mut bytes = Vec::new();
    match range {
        Some(Range::Bytes(start, len)) => {
            if len == 0 || len > LIMIT {
                return Err(EnvError::Layout(
                    "Whirlwind object exceeds the read limit".into(),
                ));
            }
            if start.checked_add(len).is_none_or(|end| end > size) {
                return Err(EnvError::Layout(format!(
                    "byte range {start}+{len} is past the end of {key} ({size} bytes)"
                )));
            }
            file.seek(SeekFrom::Start(start)).map_err(fail)?;
            bytes.resize(len as usize, 0);
            file.read_exact(&mut bytes).map_err(fail)?;
        }
        Some(Range::Suffix(len)) => {
            if len == 0 || len > LIMIT {
                return Err(EnvError::Layout(
                    "Whirlwind object exceeds the read limit".into(),
                ));
            }
            // As S3 does, a suffix longer than the file is the whole file.
            let len = len.min(size);
            file.seek(SeekFrom::Start(size - len)).map_err(fail)?;
            bytes.resize(len as usize, 0);
            file.read_exact(&mut bytes).map_err(fail)?;
        }
        None => {
            file.take(LIMIT + 1).read_to_end(&mut bytes).map_err(fail)?;
            if bytes.len() as u64 > LIMIT {
                return Err(EnvError::Layout(
                    "Whirlwind object exceeds the read limit".into(),
                ));
            }
        }
    }
    Ok(Some(Object {
        bytes: Bytes::from(bytes),
        etag: None,
    }))
}
