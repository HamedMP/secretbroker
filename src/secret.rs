use secrecy::{ExposeSecret, SecretString};

pub struct SecretValue(SecretString);

impl SecretValue {
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(SecretString::from(value))
    }

    #[must_use]
    pub(crate) fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.expose().is_empty()
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

#[cfg(test)]
mod tests {
    use super::SecretValue;

    #[test]
    fn debug_never_contains_value() {
        let value = SecretValue::new("do-not-print-me".to_owned());
        let debug = format!("{value:?}");
        assert!(!debug.contains("do-not-print-me"));
        assert!(debug.contains("REDACTED"));
    }
}
