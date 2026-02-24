use std::fmt;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::error::{OcError, OcResult};

pub type SessionId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolKind {
    Ftp,
    Sftp,
    Ftps,
}

impl fmt::Display for ProtocolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            ProtocolKind::Ftp => "ftp",
            ProtocolKind::Sftp => "sftp",
            ProtocolKind::Ftps => "ftps",
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionSecurity {
    PlainText,
    TlsExplicit,
    TlsImplicit,
    SshTransport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub protocol: ProtocolKind,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(
        default,
        serialize_with = "serialize_optional_secret",
        deserialize_with = "deserialize_optional_secret"
    )]
    pub password: Option<SecretString>,
    #[serde(
        default,
        serialize_with = "serialize_optional_secret",
        deserialize_with = "deserialize_optional_secret"
    )]
    pub private_key_pem: Option<SecretString>,
    #[serde(default)]
    pub private_key_path: Option<String>,
    pub security: ConnectionSecurity,
    pub strict_host_key_checking: bool,
    pub passive_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedConnection {
    pub name: String,
    pub profile: ConnectionProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_editor")]
    pub default_editor: String,
    #[serde(default)]
    pub connections: Vec<SavedConnection>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_editor: default_editor(),
            connections: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn normalize(mut self) -> Self {
        self.default_editor = self.default_editor.trim().to_owned();
        if self.default_editor.is_empty() {
            self.default_editor = default_editor();
        }
        self
    }
}

fn default_editor() -> String {
    "zed --wait".to_owned()
}

impl ConnectionProfile {
    pub fn socket_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn validate(&self) -> OcResult<()> {
        if self.host.trim().is_empty() {
            return Err(OcError::InvalidProfile("host cannot be empty".to_owned()));
        }

        if self.port == 0 {
            return Err(OcError::InvalidProfile("port cannot be 0".to_owned()));
        }

        if self.username.trim().is_empty() {
            return Err(OcError::InvalidProfile(
                "username cannot be empty".to_owned(),
            ));
        }

        let has_password = self.password.is_some();
        let has_private_key = self.private_key_pem.is_some()
            || self
                .private_key_path
                .as_ref()
                .is_some_and(|path| !path.trim().is_empty());

        match self.protocol {
            ProtocolKind::Ftp => {
                if self.security != ConnectionSecurity::PlainText {
                    return Err(OcError::InvalidProfile(
                        "FTP requires security=plain_text".to_owned(),
                    ));
                }
                if !has_password {
                    return Err(OcError::InvalidProfile(
                        "FTP requires password credentials".to_owned(),
                    ));
                }
            }
            ProtocolKind::Ftps => {
                if !matches!(
                    self.security,
                    ConnectionSecurity::TlsExplicit | ConnectionSecurity::TlsImplicit
                ) {
                    return Err(OcError::InvalidProfile(
                        "FTPS requires security=tls_explicit or tls_implicit".to_owned(),
                    ));
                }
                if !has_password {
                    return Err(OcError::InvalidProfile(
                        "FTPS requires password credentials".to_owned(),
                    ));
                }
            }
            ProtocolKind::Sftp => {
                if self.security != ConnectionSecurity::SshTransport {
                    return Err(OcError::InvalidProfile(
                        "SFTP requires security=ssh_transport".to_owned(),
                    ));
                }
                if !has_password && !has_private_key {
                    return Err(OcError::InvalidProfile(
                        "SFTP requires password, private_key_pem, or private_key_path".to_owned(),
                    ));
                }
            }
        }

        Ok(())
    }
}

fn serialize_optional_secret<S>(
    value: &Option<SecretString>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(secret) => serializer.serialize_some(secret.expose_secret()),
        None => serializer.serialize_none(),
    }
}

fn deserialize_optional_secret<'de, D>(deserializer: D) -> Result<Option<SecretString>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.map(SecretString::new))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteEntryKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub kind: RemoteEntryKind,
    pub size: u64,
    pub modified_unix: Option<i64>,
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::{ConnectionProfile, ConnectionSecurity, ProtocolKind};

    #[test]
    fn ftp_requires_password() {
        let profile = ConnectionProfile {
            protocol: ProtocolKind::Ftp,
            host: "localhost".to_owned(),
            port: 21,
            username: "demo".to_owned(),
            password: None,
            private_key_pem: None,
            private_key_path: None,
            security: ConnectionSecurity::PlainText,
            strict_host_key_checking: false,
            passive_mode: true,
        };

        assert!(profile.validate().is_err());
    }

    #[test]
    fn sftp_accepts_private_key_without_password() {
        let profile = ConnectionProfile {
            protocol: ProtocolKind::Sftp,
            host: "localhost".to_owned(),
            port: 22,
            username: "demo".to_owned(),
            password: None,
            private_key_pem: Some(SecretString::new("key".to_owned())),
            private_key_path: None,
            security: ConnectionSecurity::SshTransport,
            strict_host_key_checking: true,
            passive_mode: false,
        };

        assert!(profile.validate().is_ok());
    }
}
