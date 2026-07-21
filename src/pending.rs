use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tokio::time::{Instant, sleep};
use ulid::Ulid;

use crate::{
    collector::VariableRequest,
    metadata::{remove_file_if_exists, unix_timestamp},
    paths::{AppPaths, set_private_file_permissions},
    scope::Scope,
};

const REQUEST_VERSION: u32 = 1;
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestState {
    Pending,
    Ready,
    Cancelled,
    Expired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingRequest {
    pub version: u32,
    pub id: String,
    pub scope: Scope,
    pub variables: Vec<VariableRequest>,
    pub created_at: u64,
    pub expires_at: u64,
    pub state: RequestState,
}

#[derive(Clone, Debug)]
pub struct PendingStore {
    directory: PathBuf,
}

impl PendingStore {
    #[must_use]
    pub fn new(paths: &AppPaths) -> Self {
        Self {
            directory: paths.requests_dir(),
        }
    }

    pub fn create(
        &self,
        scope: Scope,
        variables: Vec<VariableRequest>,
        lifetime: Duration,
    ) -> Result<PendingRequest> {
        if variables.is_empty() {
            bail!("a pending request requires at least one variable");
        }
        self.cleanup(Duration::from_secs(24 * 60 * 60))?;
        let created_at = unix_timestamp();
        let request = PendingRequest {
            version: REQUEST_VERSION,
            id: format!("sbreq_{}", Ulid::generate()),
            scope,
            variables,
            created_at,
            expires_at: created_at.saturating_add(lifetime.as_secs()),
            state: RequestState::Pending,
        };
        self.write(&request)?;
        Ok(request)
    }

    pub fn load(&self, id: &str) -> Result<PendingRequest> {
        validate_request_id(id)?;
        let path = self.path(id);
        let reader = BufReader::new(
            File::open(&path).with_context(|| format!("pending request {id} does not exist"))?,
        );
        let mut request: PendingRequest =
            serde_json::from_reader(reader).context("pending request metadata is invalid")?;
        if request.version != REQUEST_VERSION || request.id != id {
            bail!("pending request metadata is incompatible or mismatched");
        }
        if request.state == RequestState::Pending && request.expires_at <= unix_timestamp() {
            request.state = RequestState::Expired;
            self.write(&request)?;
        }
        Ok(request)
    }

    pub fn transition(
        &self,
        id: &str,
        expected: RequestState,
        next: RequestState,
    ) -> Result<PendingRequest> {
        validate_request_id(id)?;
        let lock = self.lock(id)?;
        let mut request = self.load_unlocked(id)?;
        if request.state != expected {
            bail!(
                "request {id} is {}, expected {}",
                state_name(&request.state),
                state_name(&expected)
            );
        }
        if request.expires_at <= unix_timestamp() && next != RequestState::Expired {
            request.state = RequestState::Expired;
            self.write_unlocked(&request)?;
            drop(lock);
            bail!("request {id} has expired");
        }
        request.state = next;
        self.write_unlocked(&request)?;
        drop(lock);
        Ok(request)
    }

    pub async fn wait(&self, id: &str, timeout: Duration) -> Result<PendingRequest> {
        let deadline = Instant::now() + timeout;
        loop {
            let request = self.load(id)?;
            match request.state {
                RequestState::Ready => return Ok(request),
                RequestState::Pending if Instant::now() < deadline => sleep(POLL_INTERVAL).await,
                RequestState::Pending => return Err(anyhow!("timed out waiting for request {id}")),
                RequestState::Cancelled => return Err(anyhow!("request {id} was cancelled")),
                RequestState::Expired => return Err(anyhow!("request {id} expired")),
            }
        }
    }

    pub fn cleanup(&self, retention: Duration) -> Result<usize> {
        let now = unix_timestamp();
        let mut removed = 0;
        for entry in std::fs::read_dir(&self.directory)? {
            let path = entry?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            if validate_request_id(id).is_err() {
                continue;
            }
            let request = self.load_unlocked(id)?;
            if request.expires_at.saturating_add(retention.as_secs()) > now {
                continue;
            }
            let lock = self.lock(id)?;
            let request = self.load_unlocked(id)?;
            if request.expires_at.saturating_add(retention.as_secs()) <= unix_timestamp() {
                remove_file_if_exists(&path)?;
                removed += 1;
            }
            drop(lock);
        }
        Ok(removed)
    }

    fn load_unlocked(&self, id: &str) -> Result<PendingRequest> {
        let path = self.path(id);
        let reader = BufReader::new(
            File::open(&path).with_context(|| format!("pending request {id} does not exist"))?,
        );
        serde_json::from_reader(reader).context("pending request metadata is invalid")
    }

    fn write(&self, request: &PendingRequest) -> Result<()> {
        let lock = self.lock(&request.id)?;
        self.write_unlocked(request)?;
        drop(lock);
        Ok(())
    }

    fn write_unlocked(&self, request: &PendingRequest) -> Result<()> {
        let mut temporary = NamedTempFile::new_in(&self.directory)
            .context("cannot create pending request transaction")?;
        set_private_file_permissions(temporary.path())?;
        {
            let mut writer = BufWriter::new(temporary.as_file_mut());
            serde_json::to_writer_pretty(&mut writer, request)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
        temporary.as_file().sync_all()?;
        let destination = self.path(&request.id);
        temporary
            .persist(&destination)
            .map_err(|error| error.error)
            .with_context(|| format!("cannot commit pending request {}", request.id))?;
        set_private_file_permissions(&destination)
    }

    fn lock(&self, id: &str) -> Result<File> {
        let path = self.directory.join(format!("{id}.lock"));
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        set_private_file_permissions(&path)?;
        file.lock_exclusive()?;
        Ok(file)
    }

    fn path(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.json"))
    }
}

fn validate_request_id(id: &str) -> Result<()> {
    let ulid = id
        .strip_prefix("sbreq_")
        .ok_or_else(|| anyhow!("invalid request ID"))?;
    ulid.parse::<Ulid>().context("invalid request ID")?;
    Ok(())
}

fn state_name(state: &RequestState) -> &'static str {
    match state {
        RequestState::Pending => "pending",
        RequestState::Ready => "ready",
        RequestState::Cancelled => "cancelled",
        RequestState::Expired => "expired",
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{PendingStore, RequestState};
    use crate::{collector::VariableRequest, env_name::EnvName, paths::AppPaths, scope::Scope};

    #[test]
    fn request_file_contains_metadata_only() {
        let temporary = tempfile::tempdir().expect("temp directory");
        let paths = AppPaths::from_root(temporary.path().to_path_buf()).expect("app paths");
        let store = PendingStore::new(&paths);
        let request = store
            .create(
                Scope::user(),
                vec![VariableRequest {
                    name: EnvName::new("TOKEN").expect("name"),
                    description: Some("deployment token".to_owned()),
                }],
                Duration::from_secs(60),
            )
            .expect("create request");
        let encoded =
            std::fs::read_to_string(paths.requests_dir().join(format!("{}.json", request.id)))
                .expect("request file");
        assert!(!encoded.contains("secret_value"));
        assert!(!encoded.contains("credential_value"));
        assert_eq!(
            store.load(&request.id).expect("load").state,
            RequestState::Pending
        );
    }

    #[test]
    fn cleanup_removes_request_bodies_after_retention() {
        let temporary = tempfile::tempdir().expect("temp directory");
        let paths = AppPaths::from_root(temporary.path().to_path_buf()).expect("app paths");
        let store = PendingStore::new(&paths);
        let request = store
            .create(
                Scope::user(),
                vec![VariableRequest {
                    name: EnvName::new("TOKEN").expect("name"),
                    description: None,
                }],
                Duration::ZERO,
            )
            .expect("create request");
        assert_eq!(store.cleanup(Duration::ZERO).expect("cleanup"), 1);
        assert!(store.load(&request.id).is_err());
    }
}
