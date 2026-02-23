use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use oc_core::error::{OcError, OcResult};
use oc_core::protocol::{ProtocolFactory, ProtocolSession};
use oc_core::types::{
    ConnectionProfile, ConnectionSecurity, ProtocolKind, RemoteEntry, RemoteEntryKind,
};
use secrecy::ExposeSecret;
use ssh2::{
    CheckResult, DisconnectCode, FileType, KnownHostFileKind, RenameFlags, Session as SshSession,
    Sftp,
};

#[derive(Default)]
pub struct SftpProtocolFactory;

impl SftpProtocolFactory {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProtocolFactory for SftpProtocolFactory {
    fn kind(&self) -> ProtocolKind {
        ProtocolKind::Sftp
    }

    fn display_name(&self) -> &'static str {
        "SFTP"
    }

    async fn connect(&self, profile: &ConnectionProfile) -> OcResult<Box<dyn ProtocolSession>> {
        profile.validate()?;
        if profile.protocol != ProtocolKind::Sftp {
            return Err(OcError::InvalidProfile(
                "SFTP factory received a non-SFTP profile".to_owned(),
            ));
        }
        if profile.security != ConnectionSecurity::SshTransport {
            return Err(OcError::InvalidProfile(
                "SFTP requires ssh_transport security".to_owned(),
            ));
        }

        let mut addrs = (profile.host.as_str(), profile.port)
            .to_socket_addrs()
            .map_err(|err| OcError::Connection(format!("could not resolve host: {err}")))?;
        let socket_addr = addrs
            .next()
            .ok_or_else(|| OcError::Connection("no resolved socket addresses".to_owned()))?;

        let tcp = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(15))
            .map_err(|err| OcError::Connection(format!("could not connect TCP: {err}")))?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|err| OcError::Connection(format!("could not set read timeout: {err}")))?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(|err| OcError::Connection(format!("could not set write timeout: {err}")))?;

        let mut session = SshSession::new()
            .map_err(|err| OcError::Connection(format!("could not create ssh session: {err}")))?;
        session.set_timeout(30_000);
        session.set_tcp_stream(tcp);
        session
            .handshake()
            .map_err(|err| OcError::Connection(format!("ssh handshake failed: {err}")))?;

        verify_host_key(&session, profile)?;
        authenticate(&session, profile)?;

        if !session.authenticated() {
            return Err(OcError::Authentication);
        }

        let sftp = session
            .sftp()
            .map_err(|err| OcError::Connection(format!("could not start sftp subsystem: {err}")))?;

        Ok(Box::new(SftpSession {
            peer: profile.socket_addr(),
            session,
            sftp,
        }))
    }
}

pub struct SftpSession {
    peer: String,
    session: SshSession,
    sftp: Sftp,
}

#[async_trait]
impl ProtocolSession for SftpSession {
    fn kind(&self) -> ProtocolKind {
        ProtocolKind::Sftp
    }

    fn peer(&self) -> String {
        self.peer.clone()
    }

    async fn list_dir(&mut self, path: &str) -> OcResult<Vec<RemoteEntry>> {
        let normalized = normalize_remote_path(path);
        let entries = self
            .sftp
            .readdir(Path::new(&normalized))
            .map_err(map_ssh_error)?;

        let mut out = Vec::new();
        for (path_buf, stat) in entries {
            let name = path_buf
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }
            let kind = match stat.file_type() {
                FileType::Directory => RemoteEntryKind::Directory,
                FileType::Symlink => RemoteEntryKind::Symlink,
                _ => RemoteEntryKind::File,
            };
            out.push(RemoteEntry {
                name,
                path: path_buf.to_string_lossy().to_string(),
                kind,
                size: stat.size.unwrap_or(0),
                modified_unix: stat.mtime.map(|v| v as i64),
            });
        }
        Ok(out)
    }

    async fn upload_file(&mut self, local_path: &str, remote_path: &str) -> OcResult<()> {
        ensure_non_empty(local_path, "local_path")?;
        ensure_non_empty(remote_path, "remote_path")?;

        let mut src = File::open(local_path).map_err(|err| OcError::Io(err.to_string()))?;
        let mut dst = self
            .sftp
            .create(Path::new(remote_path))
            .map_err(map_ssh_error)?;

        let mut buf = [0_u8; 8192];
        loop {
            let n = src
                .read(&mut buf)
                .map_err(|err| OcError::Io(err.to_string()))?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n])
                .map_err(|err| OcError::Io(err.to_string()))?;
        }
        dst.flush().map_err(|err| OcError::Io(err.to_string()))?;
        Ok(())
    }

    async fn download_file(&mut self, remote_path: &str, local_path: &str) -> OcResult<()> {
        ensure_non_empty(remote_path, "remote_path")?;
        ensure_non_empty(local_path, "local_path")?;

        let mut src = self
            .sftp
            .open(Path::new(remote_path))
            .map_err(map_ssh_error)?;
        let mut dst = File::create(local_path).map_err(|err| OcError::Io(err.to_string()))?;

        let mut buf = [0_u8; 8192];
        loop {
            let n = src
                .read(&mut buf)
                .map_err(|err| OcError::Io(err.to_string()))?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n])
                .map_err(|err| OcError::Io(err.to_string()))?;
        }
        dst.flush().map_err(|err| OcError::Io(err.to_string()))?;
        Ok(())
    }

    async fn delete_path(&mut self, remote_path: &str) -> OcResult<()> {
        ensure_non_empty(remote_path, "remote_path")?;

        let path = Path::new(remote_path);
        let unlink_attempt = self.sftp.unlink(path);
        if unlink_attempt.is_ok() {
            return Ok(());
        }

        self.sftp.rmdir(path).map_err(map_ssh_error)?;
        Ok(())
    }

    async fn rename_path(&mut self, from: &str, to: &str) -> OcResult<()> {
        ensure_non_empty(from, "from")?;
        ensure_non_empty(to, "to")?;

        self.sftp
            .rename(
                Path::new(from),
                Path::new(to),
                Some(RenameFlags::OVERWRITE | RenameFlags::ATOMIC),
            )
            .map_err(map_ssh_error)?;
        Ok(())
    }

    async fn disconnect(&mut self) -> OcResult<()> {
        self.session
            .disconnect(
                Some(DisconnectCode::ByApplication),
                "Disconnect",
                Some("ostrich-connect"),
            )
            .map_err(map_ssh_error)?;
        Ok(())
    }
}

fn authenticate(session: &SshSession, profile: &ConnectionProfile) -> OcResult<()> {
    let passphrase = profile
        .password
        .as_ref()
        .map(|s| s.expose_secret().as_str());
    let username = profile.username.as_str();
    let mut errors = Vec::new();

    if let Some(private_key_pem) = profile.private_key_pem.as_ref() {
        if let Err(err) = session.userauth_pubkey_memory(
            username,
            None,
            private_key_pem.expose_secret(),
            passphrase,
        ) {
            errors.push(format!("private_key_pem auth failed: {err}"));
        } else {
            return Ok(());
        }
    }

    if let Some(private_key_path) = profile.private_key_path.as_ref() {
        if !private_key_path.trim().is_empty() {
            let key_path = expand_tilde(private_key_path);
            if let Err(err) =
                session.userauth_pubkey_file(username, None, key_path.as_path(), passphrase)
            {
                errors.push(format!(
                    "private_key_path auth failed ({}): {err}",
                    key_path.to_string_lossy()
                ));
            } else {
                return Ok(());
            }
        }
    }

    if let Some(password) = profile.password.as_ref() {
        if let Err(err) = session.userauth_password(username, password.expose_secret()) {
            errors.push(format!("password auth failed: {err}"));
        } else {
            return Ok(());
        }
    }

    if errors.is_empty() {
        Err(OcError::Authentication)
    } else {
        Err(OcError::Connection(format!(
            "authentication failed: {}",
            errors.join(" | ")
        )))
    }
}

fn verify_host_key(session: &SshSession, profile: &ConnectionProfile) -> OcResult<()> {
    let Some((host_key, host_key_type)) = session.host_key() else {
        return Err(OcError::Connection(
            "ssh server did not provide a host key".to_owned(),
        ));
    };

    let mut known_hosts = session
        .known_hosts()
        .map_err(|err| OcError::Connection(format!("known_hosts init failed: {err}")))?;

    let known_hosts_path = default_known_hosts_path();
    let read_result = known_hosts.read_file(&known_hosts_path, KnownHostFileKind::OpenSSH);

    let check = known_hosts.check_port(profile.host.as_str(), profile.port, host_key);
    match check {
        CheckResult::Match => Ok(()),
        CheckResult::Mismatch => Err(OcError::Connection(
            "host key mismatch (possible MITM)".to_owned(),
        )),
        CheckResult::NotFound => {
            if profile.strict_host_key_checking {
                Err(OcError::Connection(format!(
                    "host key not found in {}",
                    known_hosts_path.to_string_lossy()
                )))
            } else {
                let _ = read_result;
                known_hosts
                    .add(
                        format!("[{}]:{}", profile.host, profile.port).as_str(),
                        host_key,
                        profile.host.as_str(),
                        host_key_type.into(),
                    )
                    .map_err(|err| {
                        OcError::Connection(format!("could not add known host: {err}"))
                    })?;
                let _ = std::fs::create_dir_all(
                    known_hosts_path.parent().unwrap_or_else(|| Path::new(".")),
                );
                known_hosts
                    .write_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
                    .map_err(|err| {
                        OcError::Connection(format!("could not write known_hosts entry: {err}"))
                    })?;
                Ok(())
            }
        }
        CheckResult::Failure => {
            if profile.strict_host_key_checking {
                Err(OcError::Connection("host key check failed".to_owned()))
            } else {
                Ok(())
            }
        }
    }
}

fn map_ssh_error(error: ssh2::Error) -> OcError {
    OcError::Connection(error.to_string())
}

fn ensure_non_empty(value: &str, field: &str) -> OcResult<()> {
    if value.trim().is_empty() {
        return Err(OcError::InvalidCommand(format!(
            "{field} cannot be empty for this operation"
        )));
    }
    Ok(())
}

fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        "/".to_owned()
    } else if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    }
}

fn default_known_hosts_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
        .join("known_hosts")
}

fn expand_tilde(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest);
    }
    PathBuf::from(trimmed)
}
