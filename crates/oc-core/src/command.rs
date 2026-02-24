use serde::{Deserialize, Serialize};

use crate::types::{AppConfig, ConnectionProfile, ProtocolKind, RemoteEntry, SessionId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum UiCommand {
    Connect {
        profile: ConnectionProfile,
    },
    Disconnect {
        session_id: SessionId,
    },
    ListDirectory {
        session_id: SessionId,
        path: String,
    },
    UploadFile {
        session_id: SessionId,
        local_path: String,
        remote_path: String,
    },
    DownloadFile {
        session_id: SessionId,
        remote_path: String,
        local_path: String,
    },
    DeletePath {
        session_id: SessionId,
        remote_path: String,
    },
    RenamePath {
        session_id: SessionId,
        from: String,
        to: String,
    },
    LoadConfig,
    SaveConfig {
        config: AppConfig,
    },
    SupportedProtocols,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UiResponse {
    Connected {
        session_id: SessionId,
        protocol: ProtocolKind,
        peer: String,
    },
    Disconnected {
        session_id: SessionId,
    },
    Directory {
        session_id: SessionId,
        path: String,
        entries: Vec<RemoteEntry>,
    },
    TransferCompleted {
        session_id: SessionId,
        direction: TransferDirection,
        source: String,
        destination: String,
    },
    PathDeleted {
        session_id: SessionId,
        remote_path: String,
    },
    PathRenamed {
        session_id: SessionId,
        from: String,
        to: String,
    },
    Config {
        config: AppConfig,
    },
    SupportedProtocols {
        protocols: Vec<ProtocolKind>,
    },
    Ok {
        message: String,
    },
    Error {
        code: String,
        message: String,
    },
}

impl UiResponse {
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        UiResponse::Error {
            code: code.into(),
            message: message.into(),
        }
    }
}
