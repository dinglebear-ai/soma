use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{TestFailpoint, sync_parent};
use crate::{Result, UpdateError, Updater};

const MAGIC: &[u8; 8] = b"SOMAUPD1";
const DIGEST_BYTES: usize = 32;
const MAX_PATH_BYTES: usize = 16 * 1024;

pub(super) enum AuthorityWriteOutcome {
    Durable,
    RenamedIndeterminate(UpdateError),
}

pub(super) fn ensure_state_authority(
    updater: &Updater,
    authority: &Path,
    temporary: &Path,
    state: &Path,
) -> Result<()> {
    match read_state_authority(authority, temporary)? {
        Some(bound) if bound == state => Ok(()),
        Some(bound) => Err(UpdateError::InvalidLayout {
            first: bound,
            second: state.to_path_buf(),
        }),
        None => match write_authority(updater, authority, temporary, state)? {
            AuthorityWriteOutcome::Durable => Ok(()),
            AuthorityWriteOutcome::RenamedIndeterminate(error) => Err(error),
        },
    }
}

pub(super) fn read_state_authority(authority: &Path, temporary: &Path) -> Result<Option<PathBuf>> {
    let bound = read_state_authority_unconfirmed(authority, temporary)?;
    if bound.is_some() {
        sync_parent(authority)?;
    }
    Ok(bound)
}

pub(super) fn read_state_authority_unconfirmed(
    authority: &Path,
    temporary: &Path,
) -> Result<Option<PathBuf>> {
    cleanup_temporary(temporary)?;
    read_authority(authority)
}

pub(super) fn rewrite_state_authority(
    updater: &Updater,
    authority: &Path,
    temporary: &Path,
    state: &Path,
) -> Result<AuthorityWriteOutcome> {
    cleanup_temporary(temporary)?;
    write_authority(updater, authority, temporary, state)
}

fn read_authority(path: &Path) -> Result<Option<PathBuf>> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(UpdateError::io(path, error)),
    };
    let metadata = file
        .metadata()
        .map_err(|error| UpdateError::io(path, error))?;
    validate_metadata(path, &metadata)?;
    let maximum = MAGIC.len() + 4 + MAX_PATH_BYTES + DIGEST_BYTES;
    if metadata.len() > maximum as u64 {
        return Err(invalid(path, "state authority record is too large"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| UpdateError::io(path, error))?;
    decode(path, &bytes).map(Some)
}

fn write_authority(
    updater: &Updater,
    authority: &Path,
    temporary: &Path,
    state: &Path,
) -> Result<AuthorityWriteOutcome> {
    use std::os::unix::fs::OpenOptionsExt;

    let encoded = encode(state)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(temporary)
        .map_err(|error| UpdateError::io(temporary, error))?;
    let split = encoded.len().div_ceil(2);
    file.write_all(&encoded[..split])
        .map_err(|error| UpdateError::io(temporary, error))?;
    updater.maybe_fail(TestFailpoint::AuthorityAfterPartialWrite, temporary)?;
    file.write_all(&encoded[split..])
        .map_err(|error| UpdateError::io(temporary, error))?;
    updater.maybe_fail(TestFailpoint::AuthorityBeforeFileSync, temporary)?;
    file.sync_all()
        .map_err(|error| UpdateError::io(temporary, error))?;
    std::fs::rename(temporary, authority).map_err(|error| UpdateError::io(authority, error))?;
    if let Err(error) = updater.maybe_fail(TestFailpoint::AuthorityBeforeDirectorySync, authority) {
        return Ok(AuthorityWriteOutcome::RenamedIndeterminate(error));
    }
    match sync_parent(authority) {
        Ok(()) => Ok(AuthorityWriteOutcome::Durable),
        Err(error) => Ok(AuthorityWriteOutcome::RenamedIndeterminate(error)),
    }
}

fn cleanup_temporary(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(UpdateError::io(path, error)),
    };
    validate_metadata(path, &metadata)?;
    std::fs::remove_file(path).map_err(|error| UpdateError::io(path, error))?;
    sync_parent(path)
}

fn validate_metadata(path: &Path, metadata: &std::fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    if !metadata.file_type().is_file()
        || metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.mode() & 0o7777 != 0o600
    {
        return Err(invalid(
            path,
            "state authority must be an owned non-symlink regular file with mode 0600",
        ));
    }
    Ok(())
}

fn encode(state: &Path) -> Result<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;

    let path = state.as_os_str().as_bytes();
    if path.len() > MAX_PATH_BYTES {
        return Err(UpdateError::InvalidPolicy("state path is too long"));
    }
    let length = u32::try_from(path.len())
        .map_err(|_| UpdateError::InvalidPolicy("state path is too long"))?;
    let mut record = Vec::with_capacity(MAGIC.len() + 4 + path.len() + DIGEST_BYTES);
    record.extend_from_slice(MAGIC);
    record.extend_from_slice(&length.to_be_bytes());
    record.extend_from_slice(path);
    let digest = Sha256::digest(&record);
    record.extend_from_slice(&digest);
    Ok(record)
}

fn decode(record_path: &Path, record: &[u8]) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    let header = MAGIC.len() + 4;
    if record.len() < header + DIGEST_BYTES || &record[..MAGIC.len()] != MAGIC {
        return Err(invalid(
            record_path,
            "invalid state authority record header",
        ));
    }
    let length = u32::from_be_bytes(
        record[MAGIC.len()..header]
            .try_into()
            .map_err(|_| invalid(record_path, "invalid state authority record length"))?,
    ) as usize;
    if length > MAX_PATH_BYTES || record.len() != header + length + DIGEST_BYTES {
        return Err(invalid(
            record_path,
            "invalid state authority record length",
        ));
    }
    let digest_offset = header + length;
    let actual = Sha256::digest(&record[..digest_offset]);
    if actual.as_slice() != &record[digest_offset..] {
        return Err(invalid(record_path, "state authority checksum mismatch"));
    }
    Ok(PathBuf::from(std::ffi::OsString::from_vec(
        record[header..digest_offset].to_vec(),
    )))
}

fn invalid(path: &Path, message: &str) -> UpdateError {
    UpdateError::InvalidMarker {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

pub(super) fn authority_paths(executable: &Path) -> Result<(PathBuf, PathBuf)> {
    let name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(UpdateError::InvalidPolicy(
            "executable name must be valid UTF-8",
        ))?;
    let authority = executable.with_file_name(format!(".{name}.update.authority"));
    let temporary = executable.with_file_name(format!(".{name}.update.authority.tmp"));
    Ok((authority, temporary))
}

#[cfg(test)]
#[path = "transaction_authority_tests.rs"]
mod tests;
