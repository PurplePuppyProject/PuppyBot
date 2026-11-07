//! Reading the files that PuppyBot is given: its configuration and the files with secrets.

use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Reads the file at `path`, which may hold at most `limit` bytes: a larger one is an error, so a
/// wrong path, e.g. to a device, can't fill the memory. The memory is taken at once, so reading
/// leaves no copies of a secret behind.
///
/// # Errors
///
/// The error of opening or reading the file, or one of the kind
/// [`InvalidData`](io::ErrorKind::InvalidData) if it holds more than `limit` bytes.
pub fn read_limited(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let mut contents = Vec::with_capacity(limit.saturating_add(1));
    let most = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    File::open(path)?.take(most).read_to_end(&mut contents)?;
    if contents.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("larger than {limit} bytes"),
        ));
    }
    Ok(contents)
}

/// The permissions of the file at `path`, e.g. `0o644`, if users other than its owner may read
/// or change it; `None` if only its owner may, or the file can't be found.
#[must_use]
pub fn open_to_others(path: &Path) -> Option<u32> {
    let mode = fs::metadata(path).ok()?.permissions().mode() & 0o777;
    (mode & 0o077 != 0).then_some(mode)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn reads_files_up_to_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        fs::write(&path, "1234").unwrap();
        assert_eq!(read_limited(&path, 4).unwrap(), b"1234");
        assert_eq!(read_limited(&path, 100).unwrap(), b"1234");
    }

    #[test]
    fn a_larger_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        fs::write(&path, "12345").unwrap();
        let error = read_limited(&path, 4).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "larger than 4 bytes");
    }

    #[test]
    fn a_missing_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let error = read_limited(&dir.path().join("missing"), 4).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn files_open_to_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        fs::write(&path, "secret").unwrap();
        for (mode, open) in [
            (0o600, None),
            (0o400, None),
            (0o640, Some(0o640)),
            (0o644, Some(0o644)),
            (0o602, Some(0o602)),
        ] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(open_to_others(&path), open, "{mode:o}");
        }
        assert_eq!(open_to_others(&dir.path().join("missing")), None);
    }
}
