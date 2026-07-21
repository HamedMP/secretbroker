use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{env_name::EnvName, paths::set_private_file_permissions, scope::Scope};

const METADATA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub name: EnvName,
    pub scope_id: String,
    pub scope_label: String,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

impl SecretMetadata {
    #[must_use]
    pub fn is_expired_at(&self, now: u64) -> bool {
        self.expires_at.is_some_and(|expiry| expiry <= now)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MetadataDocument {
    version: u32,
    entries: Vec<SecretMetadata>,
}

impl Default for MetadataDocument {
    fn default() -> Self {
        Self {
            version: METADATA_VERSION,
            entries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MetadataStore {
    path: std::path::PathBuf,
    lock_path: std::path::PathBuf,
}

impl MetadataStore {
    #[must_use]
    pub fn new(path: std::path::PathBuf, lock_path: std::path::PathBuf) -> Self {
        Self { path, lock_path }
    }

    pub fn list(&self, scope: &Scope) -> Result<Vec<SecretMetadata>> {
        let _lock = self.lock()?;
        let mut entries: Vec<_> = self
            .load_unlocked()?
            .entries
            .into_iter()
            .filter(|entry| entry.scope_id == scope.id)
            .collect();
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    pub fn find(&self, scope: &Scope, name: &EnvName) -> Result<Option<SecretMetadata>> {
        let _lock = self.lock()?;
        Ok(self
            .load_unlocked()?
            .entries
            .into_iter()
            .find(|entry| entry.scope_id == scope.id && entry.name == *name))
    }

    pub fn upsert_many(&self, entries: Vec<SecretMetadata>) -> Result<()> {
        let _lock = self.lock()?;
        let mut document = self.load_unlocked()?;
        for entry in entries {
            document.entries.retain(|existing| {
                existing.scope_id != entry.scope_id || existing.name != entry.name
            });
            document.entries.push(entry);
        }
        self.save_unlocked(&document)
    }

    pub fn remove(&self, scope: &Scope, names: &[EnvName]) -> Result<()> {
        let _lock = self.lock()?;
        let mut document = self.load_unlocked()?;
        document.entries.retain(|entry| {
            entry.scope_id != scope.id || !names.iter().any(|name| name == &entry.name)
        });
        self.save_unlocked(&document)
    }

    fn lock(&self) -> Result<File> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&self.lock_path)
            .with_context(|| format!("cannot open metadata lock {}", self.lock_path.display()))?;
        set_private_file_permissions(&self.lock_path)?;
        file.lock_exclusive()
            .context("cannot lock secret metadata")?;
        Ok(file)
    }

    fn load_unlocked(&self) -> Result<MetadataDocument> {
        if !self.path.exists() {
            return Ok(MetadataDocument::default());
        }
        let reader = BufReader::new(
            File::open(&self.path)
                .with_context(|| format!("cannot read metadata {}", self.path.display()))?,
        );
        let document: MetadataDocument =
            serde_json::from_reader(reader).context("secret metadata is invalid")?;
        if document.version != METADATA_VERSION {
            bail!(
                "unsupported secret metadata version {}; expected {METADATA_VERSION}",
                document.version
            );
        }
        Ok(document)
    }

    fn save_unlocked(&self, document: &MetadataDocument) -> Result<()> {
        let parent = self.path.parent().context("metadata path has no parent")?;
        let mut temporary =
            NamedTempFile::new_in(parent).context("cannot create metadata transaction")?;
        set_private_file_permissions(temporary.path())?;
        {
            let mut writer = BufWriter::new(temporary.as_file_mut());
            serde_json::to_writer_pretty(&mut writer, document)
                .context("cannot encode secret metadata")?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
        temporary.as_file().sync_all()?;
        temporary
            .persist(&self.path)
            .map_err(|error| error.error)
            .with_context(|| format!("cannot commit metadata {}", self.path.display()))?;
        set_private_file_permissions(&self.path)?;
        sync_directory(parent)?;
        Ok(())
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[must_use]
pub fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("cannot remove {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::{MetadataStore, SecretMetadata};
    use crate::{env_name::EnvName, scope::Scope};

    #[test]
    fn expiry_is_inclusive() {
        let metadata = SecretMetadata {
            name: EnvName::new("TOKEN").expect("valid name"),
            scope_id: "scope".to_owned(),
            scope_label: "scope".to_owned(),
            created_at: 5,
            expires_at: Some(10),
        };
        assert!(!metadata.is_expired_at(9));
        assert!(metadata.is_expired_at(10));
    }

    #[test]
    fn serialization_has_no_secret_field() {
        let metadata = SecretMetadata {
            name: EnvName::new("TOKEN").expect("valid name"),
            scope_id: "scope".to_owned(),
            scope_label: "scope".to_owned(),
            created_at: 5,
            expires_at: None,
        };
        let encoded = serde_json::to_string(&metadata).expect("serialize metadata");
        assert!(!encoded.contains("value"));
        assert!(!encoded.contains("secret"));
    }

    #[test]
    fn repeated_commits_replace_metadata_atomically() {
        let temporary = tempfile::tempdir().expect("temp directory");
        let store = MetadataStore::new(
            temporary.path().join("metadata.json"),
            temporary.path().join("metadata.lock"),
        );
        let scope = Scope::user();
        let mut metadata = SecretMetadata {
            name: EnvName::new("TOKEN").expect("valid name"),
            scope_id: scope.id.clone(),
            scope_label: scope.label.clone(),
            created_at: 1,
            expires_at: None,
        };
        store
            .upsert_many(vec![metadata.clone()])
            .expect("first commit");
        metadata.created_at = 2;
        store.upsert_many(vec![metadata]).expect("second commit");
        let entries = store.list(&scope).expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].created_at, 2);
    }
}
