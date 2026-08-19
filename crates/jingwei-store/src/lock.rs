//! Process-scoped filesystem writer lease.

use std::{
    collections::BTreeSet,
    fs::File,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use fs2::FileExt;
use serde::Serialize;

use crate::StoreError;

/// Held exclusive workspace writer lease. Dropping it releases the OS lock.
pub struct WriterLease {
    file: File,
    process_key: PathBuf,
    /// Non-secret owner metadata path, useful for diagnostics.
    pub owner_path: PathBuf,
}

impl WriterLease {
    pub(crate) fn acquire(
        mut file: File,
        owner_path: PathBuf,
        owner: &impl Serialize,
    ) -> Result<Self, StoreError> {
        {
            let mut held = process_locks()
                .lock()
                .expect("writer lock registry poisoned");
            if !held.insert(owner_path.clone()) {
                return Err(StoreError::WriterAlreadyHeld);
            }
        }
        if let Err(error) = file.try_lock_exclusive() {
            process_locks()
                .lock()
                .expect("writer lock registry poisoned")
                .remove(&owner_path);
            return Err(if error.kind() == std::io::ErrorKind::WouldBlock {
                StoreError::WriterAlreadyHeld
            } else {
                StoreError::Io(error)
            });
        }
        let write_result = (|| -> Result<(), StoreError> {
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            serde_json::to_writer_pretty(&mut file, owner)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            Ok(())
        })();
        if let Err(error) = write_result {
            let _ = file.unlock();
            process_locks()
                .lock()
                .expect("writer lock registry poisoned")
                .remove(&owner_path);
            return Err(error);
        }
        Ok(Self {
            file,
            process_key: owner_path.clone(),
            owner_path,
        })
    }
}

impl Drop for WriterLease {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        process_locks()
            .lock()
            .expect("writer lock registry poisoned")
            .remove(&self.process_key);
    }
}

fn process_locks() -> &'static Mutex<BTreeSet<PathBuf>> {
    static LOCKS: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(BTreeSet::new()))
}
