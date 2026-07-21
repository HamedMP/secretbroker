use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        if let Some(root) = std::env::var_os("SECRETBROKER_HOME") {
            return Self::from_root(PathBuf::from(root));
        }

        let project_dirs = ProjectDirs::from("dev", "SecretBroker", "SecretBroker")
            .context("cannot determine SecretBroker application directories")?;
        Self::from_root(project_dirs.data_local_dir().to_path_buf())
    }

    pub fn from_root(root: PathBuf) -> Result<Self> {
        let paths = Self {
            data_dir: root.clone(),
            runtime_dir: root.join("runtime"),
        };
        paths.ensure_private_directories()?;
        Ok(paths)
    }

    fn ensure_private_directories(&self) -> Result<()> {
        create_private_dir(&self.data_dir)?;
        create_private_dir(&self.runtime_dir)?;
        create_private_dir(&self.runtime_dir.join("requests"))?;
        Ok(())
    }

    #[must_use]
    pub fn metadata_path(&self) -> PathBuf {
        self.data_dir.join("metadata.json")
    }

    #[must_use]
    pub fn metadata_lock_path(&self) -> PathBuf {
        self.data_dir.join("metadata.lock")
    }

    #[must_use]
    pub fn requests_dir(&self) -> PathBuf {
        self.runtime_dir.join("requests")
    }
}

fn create_private_dir(path: &std::path::Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("cannot create {}", path.display()))?;
    set_private_dir_permissions(path)
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("cannot secure {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

pub fn set_private_file_permissions(path: &std::path::Path) -> Result<()> {
    set_private_file_permissions_impl(path)
}

#[cfg(unix)]
fn set_private_file_permissions_impl(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("cannot secure {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_file_permissions_impl(_path: &std::path::Path) -> Result<()> {
    Ok(())
}
