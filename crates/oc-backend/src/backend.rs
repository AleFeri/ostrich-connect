use std::collections::HashMap;

use oc_core::command::{TransferDirection, UiCommand, UiResponse};
use oc_core::error::OcError;
use oc_core::protocol::ProtocolSession;
use oc_core::types::SessionId;
use uuid::Uuid;

use crate::ProtocolRegistry;

pub struct Backend {
    registry: ProtocolRegistry,
    sessions: HashMap<SessionId, Box<dyn ProtocolSession>>,
}

impl Backend {
    pub fn new(registry: ProtocolRegistry) -> Self {
        Self {
            registry,
            sessions: HashMap::new(),
        }
    }

    pub async fn execute(&mut self, command: UiCommand) -> UiResponse {
        match self.execute_inner(command).await {
            Ok(response) => response,
            Err(error) => UiResponse::error(error_code(&error), error.to_string()),
        }
    }

    async fn execute_inner(&mut self, command: UiCommand) -> Result<UiResponse, OcError> {
        match command {
            UiCommand::SupportedProtocols => Ok(UiResponse::SupportedProtocols {
                protocols: self.registry.supported(),
            }),
            UiCommand::Connect { profile } => {
                profile.validate()?;
                let factory = self
                    .registry
                    .get(profile.protocol)
                    .ok_or_else(|| OcError::UnsupportedProtocol(profile.protocol.to_string()))?;

                let session = factory.connect(&profile).await?;
                let peer = session.peer();
                let protocol = profile.protocol;
                let session_id = Uuid::new_v4();
                self.sessions.insert(session_id, session);

                Ok(UiResponse::Connected {
                    session_id,
                    protocol,
                    peer,
                })
            }
            UiCommand::Disconnect { session_id } => {
                let mut session = self
                    .sessions
                    .remove(&session_id)
                    .ok_or(OcError::SessionNotFound(session_id))?;

                session.disconnect().await?;

                Ok(UiResponse::Disconnected { session_id })
            }
            UiCommand::ListDirectory { session_id, path } => {
                let session = self
                    .sessions
                    .get_mut(&session_id)
                    .ok_or(OcError::SessionNotFound(session_id))?;
                let entries = session.list_dir(&path).await?;

                Ok(UiResponse::Directory {
                    session_id,
                    path,
                    entries,
                })
            }
            UiCommand::UploadFile {
                session_id,
                local_path,
                remote_path,
            } => {
                let session = self
                    .sessions
                    .get_mut(&session_id)
                    .ok_or(OcError::SessionNotFound(session_id))?;
                session.upload_file(&local_path, &remote_path).await?;

                Ok(UiResponse::TransferCompleted {
                    session_id,
                    direction: TransferDirection::Upload,
                    source: local_path,
                    destination: remote_path,
                })
            }
            UiCommand::DownloadFile {
                session_id,
                remote_path,
                local_path,
            } => {
                let session = self
                    .sessions
                    .get_mut(&session_id)
                    .ok_or(OcError::SessionNotFound(session_id))?;
                session.download_file(&remote_path, &local_path).await?;

                Ok(UiResponse::TransferCompleted {
                    session_id,
                    direction: TransferDirection::Download,
                    source: remote_path,
                    destination: local_path,
                })
            }
            UiCommand::DeletePath {
                session_id,
                remote_path,
            } => {
                let session = self
                    .sessions
                    .get_mut(&session_id)
                    .ok_or(OcError::SessionNotFound(session_id))?;
                session.delete_path(&remote_path).await?;

                Ok(UiResponse::PathDeleted {
                    session_id,
                    remote_path,
                })
            }
            UiCommand::RenamePath {
                session_id,
                from,
                to,
            } => {
                let session = self
                    .sessions
                    .get_mut(&session_id)
                    .ok_or(OcError::SessionNotFound(session_id))?;
                session.rename_path(&from, &to).await?;

                Ok(UiResponse::PathRenamed {
                    session_id,
                    from,
                    to,
                })
            }
        }
    }
}

fn error_code(error: &OcError) -> &'static str {
    match error {
        OcError::UnsupportedProtocol(_) => "unsupported_protocol",
        OcError::InvalidProfile(_) => "invalid_profile",
        OcError::InvalidCommand(_) => "invalid_command",
        OcError::Connection(_) => "connection_error",
        OcError::Authentication => "authentication_failed",
        OcError::Io(_) => "io_error",
        OcError::SessionNotFound(_) => "session_not_found",
        OcError::OperationNotSupported(_) => "operation_not_supported",
        OcError::Internal(_) => "internal_error",
    }
}
