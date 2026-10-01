//! Contained relocation with bounded, independently verified EXDEV copying.

use super::error::WritebackError;
use crate::services::epub::repack::hash_file;
use crate::services::files::RelativeFilePath;
use cap_std::fs::Dir;
use cap_tempfile::{TempDir, TempFile};
use rustix::fs::{RenameFlags, renameat_with};
use rustix::io::Errno;
use sha2::{Digest, Sha256};
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Visible relocation and its durability result.
pub enum MoveResult {
    /// Both directory updates were synchronised.
    Durable,
    /// The file moved, but a directory sync failed.
    VisibleUncertain(std::io::Error),
}

pub(super) enum Recovery {
    Destination,
    VisibleUncertain(std::io::Error),
    SourceOccupied,
    Terminal(&'static str),
}

enum Evidence {
    Verified,
    Absent,
    Changed,
}

fn evidence(
    root: &Dir,
    path: &RelativeFilePath,
    hash: &str,
    size: i64,
) -> Result<Evidence, WritebackError> {
    let mut file = match root.open(path.as_path()) {
        Ok(file) => file.into_std(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Evidence::Absent),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::other("relocation evidence is not a regular file").into());
    }
    let actual_hash = hash_file(&mut file)?;
    Ok(
        if i128::from(metadata.len()) == i128::from(size) && actual_hash == hash {
            Evidence::Verified
        } else {
            Evidence::Changed
        },
    )
}

pub(super) fn recover(
    root: &Dir,
    source: &RelativeFilePath,
    destination: &RelativeFilePath,
    hash: &str,
    size: i64,
) -> Result<Recovery, WritebackError> {
    recover_with(root, source, destination, hash, size, sync_directory)
}

fn recover_with(
    root: &Dir,
    source: &RelativeFilePath,
    destination: &RelativeFilePath,
    hash: &str,
    size: i64,
    sync: impl Fn(&Dir) -> std::io::Result<()>,
) -> Result<Recovery, WritebackError> {
    let source_evidence = evidence(root, source, hash, size);
    let destination_evidence = evidence(root, destination, hash, size);
    let source_evidence = source_evidence?;
    let destination_evidence = destination_evidence?;
    match (source_evidence, destination_evidence) {
        (Evidence::Verified, Evidence::Absent) => {
            prepare_destination(root, destination)?;
            match move_existing(root, source, destination, hash)? {
                MoveResult::Durable => Ok(Recovery::Destination),
                MoveResult::VisibleUncertain(error) => Ok(Recovery::VisibleUncertain(error)),
            }
        }
        (source_evidence, Evidence::Verified) => {
            let (destination_parent, _) = parent(root, destination)?;
            let (source_parent, source_name) = parent(root, source)?;
            sync(&destination_parent)?;
            if matches!(source_evidence, Evidence::Verified) && source != destination {
                source_parent.remove_file(source_name)?;
            }
            sync(&source_parent)?;
            Ok(Recovery::Destination)
        }
        (Evidence::Verified, Evidence::Changed) => Ok(Recovery::SourceOccupied),
        (Evidence::Absent, Evidence::Absent) => Ok(Recovery::Terminal("file_missing")),
        (Evidence::Absent, Evidence::Changed) => Ok(Recovery::Terminal(
            "file_missing: lost; destination occupied",
        )),
        (Evidence::Changed, Evidence::Absent | Evidence::Changed) => {
            Ok(Recovery::Terminal("source changed externally"))
        }
    }
}

/// Open the actual parent of a checked relative location.
///
/// # Errors
/// Returns contained lookup or missing-basename errors.
pub fn parent(root: &Dir, path: &RelativeFilePath) -> std::io::Result<(Dir, OsString)> {
    let path = path.as_path();
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let basename = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("location has no basename"))?;
    Ok((root.open_dir(directory)?, basename.to_owned()))
}

fn is_cross_device(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::CrossesDevices || error.raw_os_error() == Some(18)
}

fn sync_directory(directory: &Dir) -> std::io::Result<()> {
    directory.open(".")?.sync_all()
}

/// Create missing destination parents and sync each new entry's owning directory.
///
/// # Errors
/// Returns contained lookup, creation or parent-sync errors before relocation.
pub fn prepare_destination(root: &Dir, destination: &RelativeFilePath) -> std::io::Result<()> {
    let mut directory = root.try_clone()?;
    let mut relative = PathBuf::new();
    if let Some(parent) = destination.as_path().parent() {
        for component in parent.components() {
            match directory.create_dir(component.as_os_str()) {
                Ok(()) => sync_directory(&directory)?,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            relative.push(component);
            directory = root.open_dir(&relative)?;
        }
    }
    Ok(())
}

fn rename_no_replace(
    source: &Dir,
    name: &OsStr,
    target: &Dir,
    destination: &OsStr,
) -> std::io::Result<()> {
    renameat_with(source, name, target, destination, RenameFlags::NOREPLACE).map_err(Into::into)
}

fn commit_no_replace(
    source: &Dir,
    name: &OsStr,
    target: &Dir,
    destination: &OsStr,
) -> std::io::Result<MoveResult> {
    commit_no_replace_with(
        source,
        name,
        target,
        destination,
        rename_no_replace,
        sync_directory,
        |parent, basename| parent.remove_file(basename),
    )
}

fn commit_no_replace_with(
    source: &Dir,
    name: &OsStr,
    target: &Dir,
    destination: &OsStr,
    rename: impl FnOnce(&Dir, &OsStr, &Dir, &OsStr) -> std::io::Result<()>,
    sync: impl Fn(&Dir) -> std::io::Result<()>,
    remove: impl FnOnce(&Dir, &OsStr) -> std::io::Result<()>,
) -> std::io::Result<MoveResult> {
    match rename(source, name, target, destination) {
        Ok(()) => {}
        Err(error) if matches!(error.raw_os_error(), Some(code) if code == Errno::INVAL.raw_os_error() || code == Errno::NOSYS.raw_os_error()) =>
        {
            source.hard_link(name, target, destination)?;
            sync(target)?;
            remove(source, name)?;
            return Ok(match sync(source) {
                Ok(()) => MoveResult::Durable,
                Err(error) => MoveResult::VisibleUncertain(error),
            });
        }
        Err(error) => return Err(error),
    }
    Ok(match sync(target).and_then(|()| sync(source)) {
        Ok(()) => MoveResult::Durable,
        Err(error) => MoveResult::VisibleUncertain(error),
    })
}

fn persist(
    temp: TempFile<'_>,
    staging: &Dir,
    parent: &Dir,
    name: &OsStr,
) -> Result<(), WritebackError> {
    persist_with(temp, staging, parent, name, commit_no_replace)
}

fn persist_with(
    temp: TempFile<'_>,
    staging: &Dir,
    parent: &Dir,
    name: &OsStr,
    commit: impl FnOnce(&Dir, &OsStr, &Dir, &OsStr) -> std::io::Result<MoveResult>,
) -> Result<(), WritebackError> {
    temp.as_file().sync_all()?;
    let candidate = OsStr::new("candidate.epub");
    temp.replace(candidate)?;
    match commit(staging, candidate, parent, name)? {
        MoveResult::Durable => Ok(()),
        MoveResult::VisibleUncertain(error) => Err(error.into()),
    }
}

/// Relocate within one library, retaining the source until an EXDEV destination verifies.
///
/// # Errors
/// Non-EXDEV rename, copy, publication, verification and removal errors preserve the source where it still exists.
pub fn move_existing(
    root: &Dir,
    src: &RelativeFilePath,
    dest: &RelativeFilePath,
    expected_hash: &str,
) -> Result<MoveResult, WritebackError> {
    move_existing_with(root, src, dest, expected_hash, commit_no_replace, persist)
}

fn move_existing_with(
    root: &Dir,
    src: &RelativeFilePath,
    dest: &RelativeFilePath,
    expected_hash: &str,
    rename: impl FnOnce(&Dir, &OsStr, &Dir, &OsStr) -> std::io::Result<MoveResult>,
    publish: impl FnOnce(TempFile<'_>, &Dir, &Dir, &OsStr) -> Result<(), WritebackError>,
) -> Result<MoveResult, WritebackError> {
    let (source_parent, source_name) = parent(root, src)?;
    let (dest_parent, dest_name) = parent(root, dest)?;
    match rename(&source_parent, &source_name, &dest_parent, &dest_name) {
        Ok(movement) => return Ok(movement),
        Err(error) if !is_cross_device(&error) => return Err(error.into()),
        Err(_) => {}
    }
    let result = (|| {
        let staging = TempDir::new_in(&dest_parent)?;
        let published = (|| {
            sync_directory(&dest_parent)?;
            let mut source = source_parent.open(&source_name)?;
            let mut candidate = TempFile::new(&staging)?;
            let mut buffer = vec![0; 64 * 1024];
            let mut hash = Sha256::new();
            loop {
                let n = source.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                candidate.write_all(&buffer[..n])?;
                hash.update(&buffer[..n]);
            }
            let mut copied_hash = String::with_capacity(64);
            for byte in hash.finalize() {
                use std::fmt::Write;
                write!(copied_hash, "{byte:02x}").map_err(std::io::Error::other)?;
            }
            if copied_hash != expected_hash {
                return Err(WritebackError::Persist("source copy hash mismatch".into()));
            }
            publish(candidate, &staging, &dest_parent, &dest_name)?;
            let mut destination = dest_parent.open(&dest_name)?.into_std();
            if hash_file(&mut destination)? != expected_hash {
                return Err(WritebackError::Persist("post-copy hash mismatch".into()));
            }
            Ok::<(), WritebackError>(())
        })();
        if let Err(error) = staging.close() {
            if published.is_ok() {
                return Err(error.into());
            }
            tracing::error!(%error, "relocation staging cleanup failed");
        }
        published?;
        source_parent.remove_file(&source_name)?;
        Ok(
            match source_parent.open(".").and_then(|dir| dir.sync_all()) {
                Ok(()) => MoveResult::Durable,
                Err(error) => MoveResult::VisibleUncertain(error),
            },
        )
    })();
    if let Err(error) = &result {
        tracing::error!(destination = dest.as_str(), error = %error, "relocation failed; source retained where it still exists, destination may be published");
    }
    result
}

/// Normalise the existing template output before parsing its checked location.
///
/// # Errors
/// Returns an error for parent or absolute components.
pub fn normalise_relative(p: &Path) -> Result<PathBuf, WritebackError> {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(c) => out.push(c),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(WritebackError::Persist(format!(
                    "rendered path contains absolute component: {}",
                    p.display()
                )));
            }
            std::path::Component::ParentDir => {
                return Err(WritebackError::Persist(format!(
                    "rendered path contains ..: {}",
                    p.display()
                )));
            }
            std::path::Component::CurDir => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocation_recovery_source_only_resumes_exact_destination() {
        let (_dir, root, source, destination, hash) = fixture();
        assert!(matches!(
            recover(&root, &source, &destination, &hash, 7).unwrap(),
            Recovery::Destination
        ));
        assert!(!root.exists(source.as_path()));
        assert_eq!(root.read(destination.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn relocation_recovery_destination_only_adopts_verified_bytes() {
        let (_dir, root, source, destination, hash) = fixture();
        move_existing(&root, &source, &destination, &hash).unwrap();
        assert!(matches!(
            recover(&root, &source, &destination, &hash, 7).unwrap(),
            Recovery::Destination
        ));
        assert_eq!(root.read(destination.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn relocation_recovery_two_verified_names_remove_source_after_destination_sync() {
        let (_dir, root, source, destination, hash) = fixture();
        root.hard_link(source.as_path(), &root, destination.as_path())
            .unwrap();
        let syncs = std::cell::Cell::new(0);
        let recovered = recover_with(&root, &source, &destination, &hash, 7, |directory| {
            if syncs.get() == 0 {
                assert_eq!(root.read(source.as_path()).unwrap(), b"PAYLOAD");
            } else {
                assert!(!root.exists(source.as_path()));
            }
            assert_eq!(root.read(destination.as_path()).unwrap(), b"PAYLOAD");
            syncs.set(syncs.get() + 1);
            sync_directory(directory)
        })
        .unwrap();
        assert!(matches!(recovered, Recovery::Destination));
        assert_eq!(syncs.get(), 2);
    }

    #[test]
    fn relocation_recovery_foreign_destination_preserves_both_files() {
        let (_dir, root, source, destination, hash) = fixture();
        root.write(destination.as_path(), b"FOREIGN").unwrap();
        assert!(matches!(
            recover(&root, &source, &destination, &hash, 7).unwrap(),
            Recovery::SourceOccupied
        ));
        assert_eq!(root.read(source.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read(destination.as_path()).unwrap(), b"FOREIGN");
    }

    #[test]
    fn relocation_recovery_permanent_missing_and_changed_diagnoses() {
        for (source_bytes, destination_bytes, diagnosis) in [
            (None, None, "file_missing"),
            (
                None,
                Some(b"FOREIGN".as_slice()),
                "file_missing: lost; destination occupied",
            ),
            (
                Some(b"CHANGED".as_slice()),
                None,
                "source changed externally",
            ),
            (
                Some(b"CHANGED".as_slice()),
                Some(b"FOREIGN".as_slice()),
                "source changed externally",
            ),
        ] {
            let (_dir, root, source, destination, hash) = fixture();
            root.remove_file(source.as_path()).unwrap();
            if let Some(bytes) = source_bytes {
                root.write(source.as_path(), bytes).unwrap();
            }
            if let Some(bytes) = destination_bytes {
                root.write(destination.as_path(), bytes).unwrap();
            }
            assert!(
                matches!(recover(&root, &source, &destination, &hash, 7).unwrap(), Recovery::Terminal(actual) if actual == diagnosis)
            );
            if let Some(bytes) = source_bytes {
                assert_eq!(root.read(source.as_path()).unwrap(), bytes);
            }
            if let Some(bytes) = destination_bytes {
                assert_eq!(root.read(destination.as_path()).unwrap(), bytes);
            }
        }
    }

    #[test]
    fn relocation_recovery_changed_source_kept_with_verified_destination() {
        let (_dir, root, source, destination, hash) = fixture();
        root.write(destination.as_path(), b"PAYLOAD").unwrap();
        root.write(source.as_path(), b"CHANGED").unwrap();
        assert!(matches!(
            recover(&root, &source, &destination, &hash, 7).unwrap(),
            Recovery::Destination
        ));
        assert_eq!(root.read(source.as_path()).unwrap(), b"CHANGED");
        assert_eq!(root.read(destination.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn relocation_recovery_unreadable_evidence_precedes_loss_or_adoption() {
        for unreadable_source in [true, false] {
            let (_dir, root, source, destination, hash) = fixture();
            root.remove_file(source.as_path()).unwrap();
            if unreadable_source {
                root.create_dir(source.as_path()).unwrap();
                root.write(destination.as_path(), b"PAYLOAD").unwrap();
            } else {
                root.create_dir(destination.as_path()).unwrap();
            }
            assert!(matches!(
                recover(&root, &source, &destination, &hash, 7),
                Err(WritebackError::Io(_))
            ));
            if unreadable_source {
                assert_eq!(root.read(destination.as_path()).unwrap(), b"PAYLOAD");
            }
        }
    }

    #[test]
    fn relocation_recovery_sync_failure_preserves_evidence_and_can_resume() {
        for failing_sync in [0, 1] {
            let (_dir, root, source, destination, hash) = fixture();
            root.hard_link(source.as_path(), &root, destination.as_path())
                .unwrap();
            let syncs = std::cell::Cell::new(0);
            let result = recover_with(&root, &source, &destination, &hash, 7, |directory| {
                let count = syncs.get();
                syncs.set(count + 1);
                if count == failing_sync {
                    Err(std::io::Error::other("reported sync failure"))
                } else {
                    sync_directory(directory)
                }
            });
            assert!(matches!(result, Err(WritebackError::Io(_))));
            assert_eq!(root.exists(source.as_path()), failing_sync == 0);
            assert_eq!(root.read(destination.as_path()).unwrap(), b"PAYLOAD");
            assert!(matches!(
                recover(&root, &source, &destination, &hash, 7).unwrap(),
                Recovery::Destination
            ));
        }
    }

    #[test]
    fn relocation_recovery_size_mismatch_is_changed_even_with_matching_hash() {
        let (_dir, root, source, destination, hash) = fixture();
        assert!(matches!(
            recover(&root, &source, &destination, &hash, 8).unwrap(),
            Recovery::Terminal("source changed externally")
        ));
        assert_eq!(root.read(source.as_path()).unwrap(), b"PAYLOAD");
    }

    fn fixture() -> (
        tempfile::TempDir,
        Dir,
        RelativeFilePath,
        RelativeFilePath,
        String,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        root.write("orig.epub", b"PAYLOAD").unwrap();
        root.create_dir("subdir").unwrap();
        let mut source = root.open("orig.epub").unwrap().into_std();
        let hash = hash_file(&mut source).unwrap();
        (
            dir,
            root,
            "orig.epub".parse().unwrap(),
            "subdir/new.epub".parse().unwrap(),
            hash,
        )
    }

    #[test]
    fn no_overwrite_move_free_destination_succeeds() {
        let (_dir, root, src, dest, hash) = fixture();
        assert!(matches!(
            move_existing(&root, &src, &dest, &hash).unwrap(),
            MoveResult::Durable
        ));
        assert!(!root.exists(src.as_path()));
        assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn no_overwrite_move_existing_file_preserves_both() {
        let (_dir, root, src, dest, hash) = fixture();
        root.write(dest.as_path(), b"FOREIGN").unwrap();
        let result = move_existing(&root, &src, &dest, &hash);
        assert!(
            matches!(result, Err(WritebackError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists)
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read(dest.as_path()).unwrap(), b"FOREIGN");
    }

    #[test]
    fn no_overwrite_move_unsupported_codes_use_hard_link() {
        for code in [Errno::INVAL, Errno::NOSYS] {
            let (_dir, root, src, dest, _hash) = fixture();
            let (target, name) = parent(&root, &dest).unwrap();
            let synced = std::cell::Cell::new(0);
            let result = commit_no_replace_with(
                &root,
                src.as_path().as_os_str(),
                &target,
                &name,
                |_, _, _, _| Err(code.into()),
                |directory| {
                    assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
                    if synced.get() == 0 {
                        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
                    } else {
                        assert!(!root.exists(src.as_path()));
                    }
                    synced.set(synced.get() + 1);
                    sync_directory(directory)
                },
                |directory, name| directory.remove_file(name),
            )
            .unwrap();
            assert!(matches!(result, MoveResult::Durable));
            assert_eq!(synced.get(), 2);
            assert!(!root.exists(src.as_path()));
            assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
        }
    }

    #[test]
    fn no_overwrite_move_other_errors_never_link_or_copy() {
        for code in [
            Errno::ACCESS,
            Errno::EXIST,
            Errno::IO,
            Errno::TIMEDOUT,
            Errno::OPNOTSUPP,
        ] {
            let (_dir, root, src, dest, hash) = fixture();
            let result = move_existing_with(
                &root,
                &src,
                &dest,
                &hash,
                |source, name, target, destination| {
                    commit_no_replace_with(
                        source,
                        name,
                        target,
                        destination,
                        |_, _, _, _| Err(code.into()),
                        |_| panic!("refused move must never sync"),
                        |_, _| panic!("refused move must never remove"),
                    )
                },
                |_, _, _, _| panic!("non-EXDEV must never copy"),
            );
            assert!(
                matches!(result, Err(WritebackError::Io(error)) if error.raw_os_error() == Some(code.raw_os_error()))
            );
            assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
            assert!(!root.exists(dest.as_path()));
        }
    }

    #[test]
    fn no_overwrite_move_link_refuses_occupied_destinations() {
        for code in [Errno::INVAL, Errno::NOSYS] {
            for dangling in [false, true] {
                let (_dir, root, src, dest, _hash) = fixture();
                let (target, name) = parent(&root, &dest).unwrap();
                let result = commit_no_replace_with(
                    &root,
                    src.as_path().as_os_str(),
                    &target,
                    &name,
                    |_, _, target, name| {
                        if dangling {
                            target.symlink("missing.epub", name)?;
                        } else {
                            target.write(name, b"FOREIGN")?;
                        }
                        Err(code.into())
                    },
                    |_| panic!("refused link must never sync"),
                    |_, _| panic!("refused link must never remove"),
                );
                assert!(
                    matches!(result, Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists)
                );
                assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
                if dangling {
                    assert_eq!(
                        root.read_link(dest.as_path()).unwrap(),
                        Path::new("missing.epub")
                    );
                } else {
                    assert_eq!(root.read(dest.as_path()).unwrap(), b"FOREIGN");
                }
            }
        }
    }

    #[test]
    fn no_overwrite_move_destination_sync_failure_retains_source() {
        let (_dir, root, src, dest, _hash) = fixture();
        let (target, name) = parent(&root, &dest).unwrap();
        let result = commit_no_replace_with(
            &root,
            src.as_path().as_os_str(),
            &target,
            &name,
            |_, _, _, _| Err(Errno::INVAL.into()),
            |_| Err(Errno::IO.into()),
            |_, _| panic!("failed destination sync must never remove source"),
        );
        assert!(
            matches!(result, Err(error) if error.raw_os_error() == Some(Errno::IO.raw_os_error()))
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn no_overwrite_move_removal_failure_retains_both_names() {
        let (_dir, root, src, dest, _hash) = fixture();
        let (target, name) = parent(&root, &dest).unwrap();
        let result = commit_no_replace_with(
            &root,
            src.as_path().as_os_str(),
            &target,
            &name,
            |_, _, _, _| Err(Errno::NOSYS.into()),
            sync_directory,
            |_, _| Err(Errno::ACCESS.into()),
        );
        assert!(
            matches!(result, Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied)
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn no_overwrite_move_source_sync_failure_is_visible_uncertain() {
        for fallback in [false, true] {
            let (_dir, root, src, dest, _hash) = fixture();
            let (target, name) = parent(&root, &dest).unwrap();
            let synced = std::cell::Cell::new(0);
            let result = commit_no_replace_with(
                &root,
                src.as_path().as_os_str(),
                &target,
                &name,
                |source, name, target, destination| {
                    if fallback {
                        Err(Errno::INVAL.into())
                    } else {
                        rename_no_replace(source, name, target, destination)
                    }
                },
                |directory| {
                    synced.set(synced.get() + 1);
                    if synced.get() == 2 {
                        Err(Errno::IO.into())
                    } else {
                        sync_directory(directory)
                    }
                },
                |directory, name| directory.remove_file(name),
            )
            .unwrap();
            assert!(
                matches!(result, MoveResult::VisibleUncertain(error) if error.raw_os_error() == Some(Errno::IO.raw_os_error()))
            );
            assert!(!root.exists(src.as_path()));
            assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
        }
    }

    #[test]
    fn no_overwrite_move_prepares_nested_destination() {
        let (_dir, root, src, _, hash) = fixture();
        let destination = "new/author/book.epub".parse().unwrap();
        prepare_destination(&root, &destination).unwrap();
        prepare_destination(&root, &destination).unwrap();
        move_existing(&root, &src, &destination, &hash).unwrap();
        assert_eq!(root.read(destination.as_path()).unwrap(), b"PAYLOAD");
        assert!(!root.exists(src.as_path()));
    }

    #[test]
    fn no_overwrite_move_parent_file_refuses_before_relocation() {
        let (_dir, root, src, _, _hash) = fixture();
        root.write("occupied", b"FOREIGN").unwrap();
        assert!(prepare_destination(&root, &"occupied/book.epub".parse().unwrap()).is_err());
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read("occupied").unwrap(), b"FOREIGN");
    }

    #[test]
    fn no_overwrite_move_late_file_preserves_both() {
        let (_dir, root, src, dest, hash) = fixture();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |source, name, target, destination| {
                target.write(destination, b"FOREIGN")?;
                commit_no_replace(source, name, target, destination)
            },
            persist,
        );
        assert!(
            matches!(result, Err(WritebackError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists)
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read(dest.as_path()).unwrap(), b"FOREIGN");
    }

    #[test]
    fn no_overwrite_move_dangling_link_preserves_source() {
        let (_dir, root, src, dest, hash) = fixture();
        root.symlink("missing.epub", dest.as_path()).unwrap();
        let result = move_existing(&root, &src, &dest, &hash);
        assert!(
            matches!(result, Err(WritebackError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists)
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(
            root.read_link(dest.as_path()).unwrap(),
            Path::new("missing.epub")
        );
    }

    #[test]
    fn no_overwrite_move_prepares_internal_symlink_parent() {
        let (_dir, root, src, _, hash) = fixture();
        root.symlink("subdir", "alias").unwrap();
        let destination = "alias/nested/book.epub".parse().unwrap();
        prepare_destination(&root, &destination).unwrap();
        move_existing(&root, &src, &destination, &hash).unwrap();
        assert_eq!(root.read("subdir/nested/book.epub").unwrap(), b"PAYLOAD");
        assert!(!root.exists(src.as_path()));
    }

    #[test]
    fn no_overwrite_move_refuses_escaping_destination_parent() {
        let (dir, root, src, _, _hash) = fixture();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("escape")).unwrap();
        assert!(prepare_destination(&root, &"escape/nested/book.epub".parse().unwrap()).is_err());
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert!(!outside.path().join("nested").exists());
    }

    #[test]
    fn no_overwrite_copy_free_destination_uses_actual_parent_and_cleans_staging() {
        let (_dir, root, src, _, hash) = fixture();
        root.symlink("subdir", "alias").unwrap();
        let dest = "alias/new.epub".parse().unwrap();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(Errno::XDEV.into()),
            |temp, staging, parent, name| {
                let entries = root.read_dir("subdir")?.collect::<Result<Vec<_>, _>>()?;
                assert_eq!(entries.len(), 1);
                assert!(entries[0].file_type()?.is_dir());
                assert!(uuid::Uuid::parse_str(&entries[0].file_name().to_string_lossy()).is_ok());
                persist(temp, staging, parent, name)?;
                assert_eq!(staging.read_dir(".")?.count(), 0);
                Ok(())
            },
        )
        .unwrap();
        assert!(matches!(result, MoveResult::Durable));
        assert!(!root.exists(src.as_path()));
        assert_eq!(root.read("subdir/new.epub").unwrap(), b"PAYLOAD");
        let entries = root
            .read_dir("subdir")
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), "new.epub");
    }

    #[test]
    fn no_overwrite_copy_unsupported_codes_use_hard_link_commit() {
        for code in [Errno::INVAL, Errno::NOSYS] {
            let (_dir, root, src, dest, hash) = fixture();
            let result = move_existing_with(
                &root,
                &src,
                &dest,
                &hash,
                |_, _, _, _| Err(Errno::XDEV.into()),
                |temp, staging, parent, name| {
                    persist_with(
                        temp,
                        staging,
                        parent,
                        name,
                        |source, name, target, destination| {
                            commit_no_replace_with(
                                source,
                                name,
                                target,
                                destination,
                                |_, _, _, _| Err(code.into()),
                                sync_directory,
                                |directory, name| directory.remove_file(name),
                            )
                        },
                    )
                },
            )
            .unwrap();
            assert!(matches!(result, MoveResult::Durable));
            assert!(!root.exists(src.as_path()));
            assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
            assert_eq!(root.read_dir("subdir").unwrap().count(), 1);
        }
    }

    #[test]
    fn no_overwrite_copy_link_refuses_late_file_and_dangling_link() {
        for code in [Errno::INVAL, Errno::NOSYS] {
            for dangling in [false, true] {
                let (_dir, root, src, dest, hash) = fixture();
                let result = move_existing_with(
                    &root,
                    &src,
                    &dest,
                    &hash,
                    |_, _, _, _| Err(Errno::XDEV.into()),
                    |temp, staging, parent, name| {
                        persist_with(
                            temp,
                            staging,
                            parent,
                            name,
                            |source, name, target, destination| {
                                commit_no_replace_with(
                                    source,
                                    name,
                                    target,
                                    destination,
                                    |_, _, target, name| {
                                        if dangling {
                                            target.symlink("missing.epub", name)?;
                                        } else {
                                            target.write(name, b"FOREIGN")?;
                                        }
                                        Err(code.into())
                                    },
                                    |_| panic!("refused commit must never sync"),
                                    |_, _| panic!("refused commit must never remove"),
                                )
                            },
                        )
                    },
                );
                assert!(
                    matches!(result, Err(WritebackError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists)
                );
                assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
                if dangling {
                    assert_eq!(
                        root.read_link(dest.as_path()).unwrap(),
                        Path::new("missing.epub")
                    );
                } else {
                    assert_eq!(root.read(dest.as_path()).unwrap(), b"FOREIGN");
                }
                assert_eq!(root.read_dir("subdir").unwrap().count(), 1);
            }
        }
    }

    #[test]
    fn no_overwrite_copy_commit_sync_failure_preserves_original() {
        let (_dir, root, src, dest, hash) = fixture();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(Errno::XDEV.into()),
            |temp, staging, parent, name| {
                persist_with(
                    temp,
                    staging,
                    parent,
                    name,
                    |source, name, target, destination| {
                        commit_no_replace_with(
                            source,
                            name,
                            target,
                            destination,
                            |_, _, _, _| Err(Errno::INVAL.into()),
                            |_| Err(Errno::IO.into()),
                            |_, _| panic!("failed destination sync must never remove candidate"),
                        )
                    },
                )
            },
        );
        assert!(
            matches!(result, Err(WritebackError::Io(error)) if error.raw_os_error() == Some(Errno::IO.raw_os_error()))
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read_dir("subdir").unwrap().count(), 1);
    }

    #[test]
    fn no_overwrite_copy_late_file_preserves_both() {
        let (_dir, root, src, dest, hash) = fixture();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::CrossesDevices.into()),
            |temp, staging, parent, name| {
                parent.write(name, b"FOREIGN")?;
                persist(temp, staging, parent, name)
            },
        );
        assert!(
            matches!(result, Err(WritebackError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists)
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read(dest.as_path()).unwrap(), b"FOREIGN");
    }

    #[test]
    fn no_overwrite_copy_dangling_link_preserves_source() {
        let (_dir, root, src, dest, hash) = fixture();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::CrossesDevices.into()),
            |temp, staging, parent, name| {
                parent.symlink("missing.epub", name)?;
                persist(temp, staging, parent, name)
            },
        );
        assert!(
            matches!(result, Err(WritebackError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists)
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(
            root.read_link(dest.as_path()).unwrap(),
            Path::new("missing.epub")
        );
    }

    #[test]
    fn contained_move_same_fs_renames_atomically() {
        let (_dir, root, src, dest, hash) = fixture();
        assert!(matches!(
            move_existing(&root, &src, &dest, &hash).unwrap(),
            MoveResult::Durable
        ));
        assert!(!root.exists(src.as_path()));
        assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn contained_move_cross_fs_relocates_verified_bytes() {
        let (_dir, root, src, dest, hash) = fixture();
        move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::CrossesDevices.into()),
            persist,
        )
        .unwrap();
        assert!(!root.exists(src.as_path()));
        assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn contained_move_non_exdev_preserves_source() {
        let (_dir, root, src, dest, hash) = fixture();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::PermissionDenied.into()),
            |_, _, _, _| panic!("non-EXDEV must never copy"),
        );
        assert!(
            matches!(result, Err(WritebackError::Io(error)) if error.kind() == std::io::ErrorKind::PermissionDenied)
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert!(!root.exists(dest.as_path()));
    }

    #[test]
    fn contained_move_cross_fs_hash_mismatch_preserves_source_and_destination() {
        let (_dir, root, src, dest, hash) = fixture();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::CrossesDevices.into()),
            |temp, staging, parent, name| {
                persist(temp, staging, parent, name)?;
                parent.write(name, b"CORRUPTED")?;
                Ok(())
            },
        );
        assert!(
            matches!(result, Err(WritebackError::Persist(ref reason)) if reason == "post-copy hash mismatch")
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read(dest.as_path()).unwrap(), b"CORRUPTED");
        let retry = "subdir/new (2).epub".parse().unwrap();
        move_existing_with(
            &root,
            &src,
            &retry,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::CrossesDevices.into()),
            persist,
        )
        .unwrap();
        assert!(!root.exists(src.as_path()));
        assert_eq!(root.read(dest.as_path()).unwrap(), b"CORRUPTED");
        assert_eq!(root.read(retry.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn contained_move_cross_fs_verification_read_error_preserves_source() {
        let (_dir, root, src, dest, hash) = fixture();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::CrossesDevices.into()),
            |temp, staging, parent, name| {
                persist(temp, staging, parent, name)?;
                parent.remove_file(name)?;
                parent.symlink(name, name)?;
                Ok(())
            },
        );
        assert!(matches!(result, Err(WritebackError::Io(_))));
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert!(
            root.symlink_metadata(dest.as_path())
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn normalise_rejects_parent_dir() {
        assert!(normalise_relative(Path::new("../evil.epub")).is_err());
    }
    #[test]
    fn normalise_rejects_absolute() {
        assert!(normalise_relative(Path::new("/etc/passwd")).is_err());
    }
    #[test]
    fn normalise_strips_cur_dir() {
        assert_eq!(
            normalise_relative(Path::new("./sub/file.epub")).unwrap(),
            PathBuf::from("sub/file.epub")
        );
    }
}
