use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_ENV_NAME_LENGTH: usize = 255;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EnvName(String);

#[derive(Debug, Error)]
pub enum EnvNameError {
    #[error("environment variable name cannot be empty")]
    Empty,
    #[error("environment variable name exceeds {MAX_ENV_NAME_LENGTH} bytes")]
    TooLong,
    #[error("environment variable name must start with an ASCII letter or underscore")]
    InvalidStart,
    #[error("environment variable name may contain only ASCII letters, digits, and underscores")]
    InvalidCharacter,
}

impl EnvName {
    pub fn new(value: impl Into<String>) -> Result<Self, EnvNameError> {
        let value = value.into();
        if value.is_empty() {
            return Err(EnvNameError::Empty);
        }
        if value.len() > MAX_ENV_NAME_LENGTH {
            return Err(EnvNameError::TooLong);
        }

        let mut bytes = value.bytes();
        let first = bytes.next().ok_or(EnvNameError::Empty)?;
        if !(first.is_ascii_alphabetic() || first == b'_') {
            return Err(EnvNameError::InvalidStart);
        }
        if !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
            return Err(EnvNameError::InvalidCharacter);
        }

        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EnvName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for EnvName {
    type Err = EnvNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl From<EnvName> for String {
    fn from(value: EnvName) -> Self {
        value.0
    }
}

impl TryFrom<String> for EnvName {
    type Error = EnvNameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::EnvName;

    #[test]
    fn accepts_portable_environment_names() {
        for value in ["TOKEN", "_TOKEN", "token_2", "A"] {
            assert!(EnvName::new(value).is_ok(), "{value}");
        }
    }

    #[test]
    fn rejects_shell_syntax_and_invalid_names() {
        for value in ["", "2TOKEN", "TOKEN-VALUE", "TOKEN=value", "TÖKEN"] {
            assert!(EnvName::new(value).is_err(), "{value}");
        }
    }
}
