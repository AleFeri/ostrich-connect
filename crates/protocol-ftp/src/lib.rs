use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use oc_core::error::{OcError, OcResult};
use oc_core::protocol::{ProtocolFactory, ProtocolSession};
use oc_core::types::{
    ConnectionProfile, ConnectionSecurity, ProtocolKind, RemoteEntry, RemoteEntryKind,
};
use secrecy::ExposeSecret;
use suppaftp::list::File as FtpListFile;
use suppaftp::{FtpError, FtpStream, Mode};

#[derive(Default)]
pub struct FtpProtocolFactory;

impl FtpProtocolFactory {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProtocolFactory for FtpProtocolFactory {
    fn kind(&self) -> ProtocolKind {
        ProtocolKind::Ftp
    }

    fn display_name(&self) -> &'static str {
        "FTP"
    }

    async fn connect(&self, profile: &ConnectionProfile) -> OcResult<Box<dyn ProtocolSession>> {
        profile.validate()?;
        if profile.protocol != ProtocolKind::Ftp {
            return Err(OcError::InvalidProfile(
                "FTP factory received a non-FTP profile".to_owned(),
            ));
        }
        if profile.security != ConnectionSecurity::PlainText {
            return Err(OcError::InvalidProfile(
                "FTP only accepts plain_text security".to_owned(),
            ));
        }

        let password = profile.password.as_ref().ok_or_else(|| {
            OcError::InvalidProfile("FTP requires password credentials".to_owned())
        })?;
        let addr = profile.socket_addr();

        let mut stream = FtpStream::connect(addr.as_str()).map_err(map_ftp_error)?;
        if profile.passive_mode {
            stream.set_mode(Mode::Passive);
        } else {
            stream.set_mode(Mode::Active);
        }
        stream
            .login(&profile.username, password.expose_secret())
            .map_err(map_ftp_error)?;

        Ok(Box::new(FtpSession {
            peer: addr,
            stream,
            cwd: "/".to_owned(),
        }))
    }
}

pub struct FtpSession {
    peer: String,
    stream: FtpStream,
    cwd: String,
}

#[async_trait]
impl ProtocolSession for FtpSession {
    fn kind(&self) -> ProtocolKind {
        ProtocolKind::Ftp
    }

    fn peer(&self) -> String {
        self.peer.clone()
    }

    async fn list_dir(&mut self, path: &str) -> OcResult<Vec<RemoteEntry>> {
        let normalized = normalize_remote_path(path);
        self.stream.cwd(&normalized).map_err(map_ftp_error)?;
        self.cwd = normalized.clone();

        let lines = self.stream.list(None).map_err(map_ftp_error)?;
        let mut entries = Vec::new();
        for line in lines {
            let parsed = FtpListFile::try_from(line.as_str());
            if let Ok(file) = parsed {
                let name = file.name().to_owned();
                let full_path = join_remote_path(&normalized, &name);
                let kind = if file.is_directory() {
                    RemoteEntryKind::Directory
                } else if file.is_symlink() {
                    RemoteEntryKind::Symlink
                } else {
                    RemoteEntryKind::File
                };
                let modified_unix = file
                    .modified()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .and_then(|d| i64::try_from(d.as_secs()).ok());

                entries.push(RemoteEntry {
                    name,
                    path: full_path,
                    kind,
                    size: file.size() as u64,
                    modified_unix,
                });
            }
        }
        Ok(entries)
    }

    async fn upload_file(&mut self, local_path: &str, remote_path: &str) -> OcResult<()> {
        ensure_non_empty(local_path, "local_path")?;
        ensure_non_empty(remote_path, "remote_path")?;

        let remote_name = basename(remote_path)?;
        let remote_dir = parent_remote_dir(remote_path);
        self.stream.cwd(&remote_dir).map_err(map_ftp_error)?;
        self.cwd = remote_dir;

        let mut local_file = File::open(local_path).map_err(|err| OcError::Io(err.to_string()))?;
        self.stream
            .put_file(remote_name.as_str(), &mut local_file)
            .map_err(map_ftp_error)?;
        Ok(())
    }

    async fn download_file(&mut self, remote_path: &str, local_path: &str) -> OcResult<()> {
        ensure_non_empty(remote_path, "remote_path")?;
        ensure_non_empty(local_path, "local_path")?;

        let remote_name = basename(remote_path)?;
        let remote_dir = parent_remote_dir(remote_path);
        self.stream.cwd(&remote_dir).map_err(map_ftp_error)?;
        self.cwd = remote_dir;

        let mut file = File::create(local_path).map_err(|err| OcError::Io(err.to_string()))?;
        let cursor = self
            .stream
            .retr_as_buffer(remote_name.as_str())
            .map_err(map_ftp_error)?;
        file.write_all(cursor.get_ref())
            .map_err(|err| OcError::Io(err.to_string()))?;
        Ok(())
    }

    async fn delete_path(&mut self, remote_path: &str) -> OcResult<()> {
        ensure_non_empty(remote_path, "remote_path")?;

        let target = normalize_remote_path(remote_path);
        let file_delete = self.stream.rm(&target);
        if file_delete.is_ok() {
            return Ok(());
        }

        self.stream.rmdir(&target).map_err(map_ftp_error)?;
        Ok(())
    }

    async fn rename_path(&mut self, from: &str, to: &str) -> OcResult<()> {
        ensure_non_empty(from, "from")?;
        ensure_non_empty(to, "to")?;

        self.stream.rename(from, to).map_err(map_ftp_error)?;
        Ok(())
    }

    async fn disconnect(&mut self) -> OcResult<()> {
        self.stream.quit().map_err(map_ftp_error)?;
        Ok(())
    }
}

fn map_ftp_error(error: FtpError) -> OcError {
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

fn join_remote_path(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{}/{}", dir.trim_end_matches('/'), name)
    }
}

fn basename(path: &str) -> OcResult<String> {
    let normalized = normalize_remote_path(path);
    let file_name = Path::new(&normalized)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if file_name.is_empty() {
        return Err(OcError::InvalidCommand(
            "path must include a file name".to_owned(),
        ));
    }
    Ok(file_name)
}

fn parent_remote_dir(path: &str) -> String {
    let normalized = normalize_remote_path(path);
    let as_path = PathBuf::from(normalized);
    let parent = as_path.parent().unwrap_or_else(|| Path::new("/"));
    let as_text = parent.to_string_lossy().to_string();
    if as_text.is_empty() {
        "/".to_owned()
    } else if as_text.starts_with('/') {
        as_text
    } else {
        format!("/{as_text}")
    }
}
