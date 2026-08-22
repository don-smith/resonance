use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use fs2::FileExt;
use resonance_runtime::identity::FileKeyCustody;

use crate::startup::DebugProfileName;

const PROFILE_LOCK_FILE: &str = ".profile.lock";

pub(crate) struct DebugProfile {
    custody: FileKeyCustody,
    application_data: PathBuf,
    lock: File,
    lock_path: PathBuf,
}

#[derive(Debug)]
pub(crate) enum DebugProfileError {
    Unavailable,
    Active,
}

impl fmt::Display for DebugProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => {
                formatter.write_str("the selected debug profile cannot be opened safely")
            }
            Self::Active => formatter.write_str("the selected debug profile is already active"),
        }
    }
}

impl std::error::Error for DebugProfileError {}

impl DebugProfile {
    pub(crate) fn open(name: &DebugProfileName) -> Result<Self, DebugProfileError> {
        Self::open_at(&repository_root(), name)
    }

    fn open_at(root: &Path, name: &DebugProfileName) -> Result<Self, DebugProfileError> {
        let resonance_root = root.join(".resonance");
        ensure_private_directory(&resonance_root)?;
        let profiles_root = resonance_root.join("debug-profiles");
        ensure_private_directory(&profiles_root)?;
        let profile_root = profiles_root.join(name.as_str());
        ensure_private_directory(&profile_root)?;

        let lock_path = profile_root.join(PROFILE_LOCK_FILE);
        let mut lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)
            .map_err(|_| DebugProfileError::Unavailable)?;
        lock.try_lock_exclusive()
            .map_err(|_| DebugProfileError::Active)?;
        lock.set_len(0)
            .map_err(|_| DebugProfileError::Unavailable)?;
        write!(lock, "{}\n", std::process::id()).map_err(|_| DebugProfileError::Unavailable)?;
        lock.sync_all()
            .map_err(|_| DebugProfileError::Unavailable)?;

        let identity_directory = profile_root.join("identity");
        ensure_private_directory(&identity_directory)?;
        let application_data = profile_root.join("app-data");
        ensure_private_directory(&application_data)?;
        let custody =
            FileKeyCustody::open(identity_directory).map_err(|_| DebugProfileError::Unavailable)?;

        Ok(Self {
            custody,
            application_data,
            lock,
            lock_path,
        })
    }

    pub(crate) fn custody(&self) -> &FileKeyCustody {
        &self.custody
    }

    pub(crate) fn application_data(&self) -> &Path {
        &self.application_data
    }
}

impl Drop for DebugProfile {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.lock);
        let _ = fs::remove_file(&self.lock_path);
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("desktop crate is nested under the repository root")
        .to_path_buf()
}

fn ensure_private_directory(path: &Path) -> Result<(), DebugProfileError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::create_dir(path)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(|_| DebugProfileError::Unavailable)?;
        let metadata = fs::symlink_metadata(path).map_err(|_| DebugProfileError::Unavailable)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(DebugProfileError::Unavailable);
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| DebugProfileError::Unavailable)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(DebugProfileError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{DebugProfile, DebugProfileError};
    use crate::startup::DebugProfileName;
    use resonance_runtime::identity::InstallationIdentity;

    fn name(value: &str) -> DebugProfileName {
        DebugProfileName::parse(value).expect("valid profile name")
    }

    #[test]
    fn profiles_are_locked_and_keep_separate_data_roots() {
        let root = tempfile::tempdir().expect("temporary root creates");
        let alice = DebugProfile::open_at(root.path(), &name("alice")).expect("alice opens");
        assert!(matches!(
            DebugProfile::open_at(root.path(), &name("alice")),
            Err(DebugProfileError::Active)
        ));
        let bob = DebugProfile::open_at(root.path(), &name("bob")).expect("bob opens");

        assert_ne!(alice.application_data(), bob.application_data());
        assert_ne!(
            InstallationIdentity::load_or_create(alice.custody())
                .expect("alice identity creates")
                .public_identity(),
            InstallationIdentity::load_or_create(bob.custody())
                .expect("bob identity creates")
                .public_identity()
        );
        assert!(alice.application_data().ends_with("alice/app-data"));
        drop(alice);
        DebugProfile::open_at(root.path(), &name("alice")).expect("released profile opens");
    }

    #[test]
    fn refuses_a_symlink_profile_root() {
        let root = tempfile::tempdir().expect("temporary root creates");
        let profiles = root.path().join(".resonance/debug-profiles");
        fs::create_dir_all(&profiles).expect("profiles root creates");
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.path(), profiles.join("alice")).expect("symlink creates");

        assert!(matches!(
            DebugProfile::open_at(root.path(), &name("alice")),
            Err(DebugProfileError::Unavailable)
        ));
    }
}
