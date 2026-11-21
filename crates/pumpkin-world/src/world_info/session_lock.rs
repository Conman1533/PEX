use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
};

use super::WorldInfoError;

pub const LOCK_FILE: &str = "session.lock";

/// Vanilla's `DirectoryLock.DUMMY`, written into the file before locking it.
const DUMMY: &str = "\u{2603}";

/// `session.lock` in a world folder, held like vanilla's `DirectoryLock` so PEX and another
/// server never write one world at the same time. The lock goes with the value.
#[derive(Debug)]
pub struct SessionLock {
    _file: File,
}

impl SessionLock {
    /// Takes the world's lock, like `DirectoryLock.create`.
    pub fn create(dir: &Path) -> Result<Self, WorldInfoError> {
        let path = dir.join(LOCK_FILE);
        std::fs::create_dir_all(dir)?;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        file.write_all(DUMMY.as_bytes())?;
        file.sync_all()?;
        if try_lock(&file)? {
            Ok(Self { _file: file })
        } else {
            Err(WorldInfoError::SessionLocked(path))
        }
    }
}

/// Java's `FileChannel.tryLock` takes an `fcntl` record lock on Unix, which `flock` (what
/// `File::try_lock` uses there) doesn't conflict with, so take the same kind of lock. Linux
/// gets an open file description lock, which also conflicts with a second lock in this process.
#[cfg(unix)]
fn try_lock(file: &File) -> io::Result<bool> {
    use std::os::fd::AsRawFd;

    #[cfg(any(target_os = "linux", target_os = "android"))]
    const SET_LOCK: libc::c_int = libc::F_OFD_SETLK;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    const SET_LOCK: libc::c_int = libc::F_SETLK;

    // SAFETY: `flock` is a plain C struct, for which all zero bytes is a valid value: the whole
    // file (`l_start` and `l_len` 0) from `SEEK_SET` (0), and `l_pid` 0 as OFD locks require.
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = libc::F_WRLCK as _;
    // SAFETY: the descriptor stays open for the call and `lock` is a valid `flock`.
    if unsafe { libc::fcntl(file.as_raw_fd(), SET_LOCK, &raw const lock) } == 0 {
        return Ok(true);
    }
    let err = io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::EACCES | libc::EAGAIN) => Ok(false),
        _ => Err(err),
    }
}

/// On Windows both Java and `File::try_lock` use `LockFileEx`.
#[cfg(not(unix))]
fn try_lock(file: &File) -> io::Result<bool> {
    match file.try_lock() {
        Ok(()) => Ok(true),
        Err(std::fs::TryLockError::WouldBlock) => Ok(false),
        Err(std::fs::TryLockError::Error(err)) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // POSIX record locks don't conflict inside one process; Linux OFD and Windows locks do.
    #[cfg(any(not(unix), target_os = "linux", target_os = "android"))]
    #[test]
    fn a_locked_world_is_refused_until_its_lock_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let lock = SessionLock::create(dir.path()).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join(LOCK_FILE)).unwrap(),
            DUMMY.as_bytes()
        );
        assert!(matches!(
            SessionLock::create(dir.path()),
            Err(WorldInfoError::SessionLocked(_))
        ));
        drop(lock);
        assert!(SessionLock::create(dir.path()).is_ok());
    }
}
