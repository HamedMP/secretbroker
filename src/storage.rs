use anyhow::{Context, Result, anyhow};

use crate::{
    collector::CollectedSecret,
    env_name::EnvName,
    metadata::{MetadataStore, SecretMetadata, unix_timestamp},
    scope::Scope,
    secret::SecretValue,
};

const KEYRING_SERVICE: &str = "dev.secretbroker.cli";

pub trait CredentialStore: Send + Sync {
    fn put(&self, scope: &Scope, name: &EnvName, value: &SecretValue) -> Result<()>;
    fn get(&self, scope: &Scope, name: &EnvName) -> Result<Option<SecretValue>>;
    fn delete(&self, scope: &Scope, name: &EnvName) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeCredentialStore;

impl NativeCredentialStore {
    fn entry(scope: &Scope, name: &EnvName) -> Result<keyring::Entry> {
        let username = format!("v1:{}:{}", scope.credential_namespace(), name.as_str());
        keyring::Entry::new(KEYRING_SERVICE, &username).context("cannot access OS credential store")
    }
}

impl CredentialStore for NativeCredentialStore {
    fn put(&self, scope: &Scope, name: &EnvName, value: &SecretValue) -> Result<()> {
        Self::entry(scope, name)?
            .set_password(value.expose())
            .with_context(|| format!("cannot store credential {name}"))
    }

    fn get(&self, scope: &Scope, name: &EnvName) -> Result<Option<SecretValue>> {
        match Self::entry(scope, name)?.get_password() {
            Ok(value) => Ok(Some(SecretValue::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error).with_context(|| format!("cannot read credential {name}")),
        }
    }

    fn delete(&self, scope: &Scope, name: &EnvName) -> Result<()> {
        match Self::entry(scope, name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error).with_context(|| format!("cannot delete credential {name}")),
        }
    }
}

pub struct SecretRepository<S> {
    credentials: S,
    metadata: MetadataStore,
}

impl<S: CredentialStore> SecretRepository<S> {
    #[must_use]
    pub fn new(credentials: S, metadata: MetadataStore) -> Self {
        Self {
            credentials,
            metadata,
        }
    }

    pub fn store_many(
        &self,
        scope: &Scope,
        values: Vec<CollectedSecret>,
        expires_at: Option<u64>,
    ) -> Result<Vec<SecretMetadata>> {
        if values.is_empty() {
            return Ok(Vec::new());
        }

        let mut previous = Vec::with_capacity(values.len());
        for (name, _) in &values {
            previous.push((name.clone(), self.credentials.get(scope, name)?));
        }

        for (written, (name, value)) in values.iter().enumerate() {
            if let Err(error) = self.credentials.put(scope, name, value) {
                self.rollback(scope, &previous[..=written]);
                return Err(error);
            }
        }

        let now = unix_timestamp();
        let metadata: Vec<_> = values
            .iter()
            .map(|(name, _)| SecretMetadata {
                name: name.clone(),
                scope_id: scope.id.clone(),
                scope_label: scope.label.clone(),
                created_at: now,
                expires_at,
            })
            .collect();

        if let Err(error) = self.metadata.upsert_many(metadata.clone()) {
            self.rollback(scope, &previous);
            return Err(error).context("credentials were rolled back after metadata commit failed");
        }

        Ok(metadata)
    }

    pub fn resolve(&self, scope: &Scope, name: &EnvName) -> Result<SecretValue> {
        let metadata = self
            .metadata
            .find(scope, name)?
            .ok_or_else(|| anyhow!("credential {name} is not available in {scope}"))?;
        if metadata.is_expired_at(unix_timestamp()) {
            self.credentials.delete(scope, name)?;
            self.metadata.remove(scope, std::slice::from_ref(name))?;
            return Err(anyhow!("credential {name} has expired in {scope}"));
        }
        self.credentials.get(scope, name)?.ok_or_else(|| {
            anyhow!("credential {name} metadata exists but the OS credential is missing")
        })
    }

    pub fn status(&self, scope: &Scope) -> Result<Vec<SecretMetadata>> {
        self.metadata.list(scope)
    }

    pub fn delete(&self, scope: &Scope, names: &[EnvName]) -> Result<()> {
        for name in names {
            self.credentials.delete(scope, name)?;
        }
        self.metadata.remove(scope, names)
    }

    pub fn clear(&self, scope: &Scope) -> Result<usize> {
        let entries = self.metadata.list(scope)?;
        let names: Vec<_> = entries.into_iter().map(|entry| entry.name).collect();
        self.delete(scope, &names)?;
        Ok(names.len())
    }

    fn rollback(&self, scope: &Scope, previous: &[(EnvName, Option<SecretValue>)]) {
        for (name, value) in previous.iter().rev() {
            match value {
                Some(value) => {
                    let _ = self.credentials.put(scope, name, value);
                }
                None => {
                    let _ = self.credentials.delete(scope, name);
                }
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use super::{CredentialStore, NativeCredentialStore, SecretRepository};
    use crate::{env_name::EnvName, metadata::MetadataStore, scope::Scope, secret::SecretValue};
    use anyhow::Result;

    #[derive(Default)]
    pub struct MemoryCredentialStore(Mutex<HashMap<(String, String), String>>);

    impl CredentialStore for MemoryCredentialStore {
        fn put(&self, scope: &Scope, name: &EnvName, value: &SecretValue) -> Result<()> {
            self.0.lock().expect("lock").insert(
                (scope.id.clone(), name.as_str().to_owned()),
                value.expose().to_owned(),
            );
            Ok(())
        }

        fn get(&self, scope: &Scope, name: &EnvName) -> Result<Option<SecretValue>> {
            Ok(self
                .0
                .lock()
                .expect("lock")
                .get(&(scope.id.clone(), name.as_str().to_owned()))
                .cloned()
                .map(SecretValue::new))
        }

        fn delete(&self, scope: &Scope, name: &EnvName) -> Result<()> {
            self.0
                .lock()
                .expect("lock")
                .remove(&(scope.id.clone(), name.as_str().to_owned()));
            Ok(())
        }
    }

    #[test]
    fn repository_resolves_values_without_writing_them_to_metadata() {
        let temporary = tempfile::tempdir().expect("temp directory");
        let metadata_path = temporary.path().join("metadata.json");
        let repository = SecretRepository::new(
            MemoryCredentialStore::default(),
            MetadataStore::new(
                metadata_path.clone(),
                temporary.path().join("metadata.lock"),
            ),
        );
        let scope = Scope::user();
        let name = EnvName::new("TOKEN").expect("name");
        repository
            .store_many(
                &scope,
                vec![(
                    name.clone(),
                    SecretValue::new("synthetic-secret".to_owned()),
                )],
                None,
            )
            .expect("store");
        assert_eq!(
            repository.resolve(&scope, &name).expect("resolve").expose(),
            "synthetic-secret"
        );
        let metadata = std::fs::read_to_string(metadata_path).expect("metadata");
        assert!(!metadata.contains("synthetic-secret"));
    }

    #[test]
    #[ignore = "uses the current user's operating system credential store"]
    fn native_store_round_trip() {
        let scope =
            Scope::session(&format!("native-test-{}", ulid::Ulid::generate())).expect("test scope");
        let name = EnvName::new("SECRETBROKER_TEST_VALUE").expect("name");
        let store = NativeCredentialStore;
        store
            .put(
                &scope,
                &name,
                &SecretValue::new("synthetic-only".to_owned()),
            )
            .expect("put native credential");
        let value = store
            .get(&scope, &name)
            .expect("get native credential")
            .expect("credential exists");
        assert_eq!(value.expose(), "synthetic-only");
        store
            .delete(&scope, &name)
            .expect("delete native credential");
        assert!(store.get(&scope, &name).expect("verify deletion").is_none());
    }
}
