use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use thiserror::Error;
use zeroize::Zeroizing;

const SERVICE_NAME: &str = "dev.nummetria.cli";
const REDACTED: &str = "[REDACTED]";

/// A validated provider/profile identity used as a native credential key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CredentialId {
    provider: String,
    profile: String,
}

impl CredentialId {
    pub fn new(
        provider: impl Into<String>,
        profile: impl Into<String>,
    ) -> Result<Self, SecretError> {
        let provider = provider.into();
        let profile = profile.into();
        validate_component("provider", &provider)?;
        validate_component("profile", &profile)?;
        Ok(Self { provider, profile })
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    fn account(&self) -> String {
        format!("{}:{}", self.provider, self.profile)
    }
}

/// Secret text that zeroes memory on drop and redacts all formatting.
pub struct SecretValue(Zeroizing<String>);

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Result<Self, SecretError> {
        let value = value.into();
        if value.is_empty() {
            return Err(SecretError::EmptySecret);
        }
        Ok(Self(Zeroizing::new(value)))
    }

    /// Deliberately exposes the secret to an authenticated provider request.
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("SecretValue")
            .field(&REDACTED)
            .finish()
    }
}

impl fmt::Display for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("credential {field} must contain only letters, numbers, '.', '_', or '-'")]
    InvalidCredentialId { field: &'static str },
    #[error("credential value cannot be empty")]
    EmptySecret,
    #[error("native credential store operation failed: {0}")]
    Backend(String),
}

pub trait SecretStore: Send + Sync {
    fn set(&self, id: &CredentialId, value: &SecretValue) -> Result<(), SecretError>;
    fn get(&self, id: &CredentialId) -> Result<Option<SecretValue>, SecretError>;
    fn delete(&self, id: &CredentialId) -> Result<bool, SecretError>;
}

/// macOS Keychain / Windows Credential Manager adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn set(&self, id: &CredentialId, value: &SecretValue) -> Result<(), SecretError> {
        entry(id)?
            .set_password(value.expose_secret())
            .map_err(backend_error)
    }

    fn get(&self, id: &CredentialId) -> Result<Option<SecretValue>, SecretError> {
        match entry(id)?.get_password() {
            Ok(value) => SecretValue::new(value).map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(backend_error(error)),
        }
    }

    fn delete(&self, id: &CredentialId) -> Result<bool, SecretError> {
        match entry(id)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(backend_error(error)),
        }
    }
}

/// Deterministic credential store for tests; never touches native storage.
#[derive(Debug, Default)]
pub struct InMemorySecretStore {
    values: Mutex<HashMap<CredentialId, String>>,
}

impl SecretStore for InMemorySecretStore {
    fn set(&self, id: &CredentialId, value: &SecretValue) -> Result<(), SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError::Backend("in-memory credential store lock failed".into()))?
            .insert(id.clone(), value.expose_secret().to_owned());
        Ok(())
    }

    fn get(&self, id: &CredentialId) -> Result<Option<SecretValue>, SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError::Backend("in-memory credential store lock failed".into()))?
            .get(id)
            .cloned()
            .map(SecretValue::new)
            .transpose()
    }

    fn delete(&self, id: &CredentialId) -> Result<bool, SecretError> {
        Ok(self
            .values
            .lock()
            .map_err(|_| SecretError::Backend("in-memory credential store lock failed".into()))?
            .remove(id)
            .is_some())
    }
}

fn entry(id: &CredentialId) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(SERVICE_NAME, &id.account()).map_err(backend_error)
}

fn backend_error(error: keyring::Error) -> SecretError {
    SecretError::Backend(error.to_string())
}

fn validate_component(field: &'static str, value: &str) -> Result<(), SecretError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(SecretError::InvalidCredentialId { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_credential_identity_components() {
        assert!(CredentialId::new("openai", "default").is_ok());
        assert!(CredentialId::new("", "default").is_err());
        assert!(CredentialId::new("openai:admin", "default").is_err());
        assert!(CredentialId::new("openai", "two words").is_err());
    }

    #[test]
    fn secret_formatting_is_always_redacted() {
        let raw = "sk-example-must-never-appear";
        let secret = SecretValue::new(raw).unwrap();
        assert_eq!(secret.to_string(), REDACTED);
        assert!(!format!("{secret:?}").contains(raw));
        assert!(format!("{secret:?}").contains(REDACTED));
        assert_eq!(secret.expose_secret(), raw);
    }

    #[test]
    fn in_memory_store_matches_set_get_delete_contract() {
        let store = InMemorySecretStore::default();
        let id = CredentialId::new("openai", "default").unwrap();
        let value = SecretValue::new("private-value").unwrap();

        assert!(store.get(&id).unwrap().is_none());
        store.set(&id, &value).unwrap();
        assert_eq!(
            store.get(&id).unwrap().unwrap().expose_secret(),
            "private-value"
        );
        assert!(store.delete(&id).unwrap());
        assert!(!store.delete(&id).unwrap());
        assert!(store.get(&id).unwrap().is_none());
    }
}
