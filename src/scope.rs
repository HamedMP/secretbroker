use std::{fmt, path::Path};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub kind: ScopeKind,
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    User,
    Project,
    Session,
}

impl Scope {
    #[must_use]
    pub fn user() -> Self {
        Self {
            kind: ScopeKind::User,
            id: "user".to_owned(),
            label: "user".to_owned(),
        }
    }

    pub fn project(path: &Path) -> Result<Self> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("cannot resolve project directory {}", path.display()))?;
        let path_bytes = canonical.to_string_lossy();
        let digest = Sha256::digest(path_bytes.as_bytes());
        let id = format!("project:{}", hex::encode(digest));
        Ok(Self {
            kind: ScopeKind::Project,
            id,
            label: format!("project:{}", canonical.display()),
        })
    }

    pub fn session(id: &str) -> Result<Self> {
        if id.is_empty() || id.len() > 128 {
            bail!("session ID must contain between 1 and 128 characters");
        }
        if !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            bail!(
                "session ID may contain only ASCII letters, digits, dots, hyphens, and underscores"
            );
        }
        let digest = Sha256::digest(id.as_bytes());
        Ok(Self {
            kind: ScopeKind::Session,
            id: format!("session:{}", hex::encode(digest)),
            label: format!("session:{id}"),
        })
    }

    pub fn parse(value: &str, project_dir: &Path) -> Result<Self> {
        match value {
            "user" => Ok(Self::user()),
            "project" => Self::project(project_dir),
            _ => value.strip_prefix("session:").map_or_else(
                || bail!("scope must be user, project, or session:<id>"),
                Self::session,
            ),
        }
    }

    #[must_use]
    pub fn credential_namespace(&self) -> String {
        let digest = Sha256::digest(self.id.as_bytes());
        hex::encode(digest)
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

#[cfg(test)]
mod tests {
    use super::Scope;

    #[test]
    fn user_scope_is_stable() {
        assert_eq!(Scope::user().id, "user");
        assert_eq!(Scope::user().credential_namespace().len(), 64);
    }

    #[test]
    fn session_scope_does_not_expose_input_in_storage_id() {
        let scope = Scope::session("private-session-name").expect("valid scope");
        assert!(!scope.id.contains("private-session-name"));
    }

    #[test]
    fn rejects_unsafe_session_ids() {
        assert!(Scope::session("../../other").is_err());
        assert!(Scope::session("").is_err());
    }
}
