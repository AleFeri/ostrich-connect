use async_trait::async_trait;

use crate::error::OcResult;
use crate::types::{ConnectionProfile, ProtocolKind, RemoteEntry};

#[async_trait]
pub trait ProtocolFactory: Send + Sync {
    fn kind(&self) -> ProtocolKind;
    fn display_name(&self) -> &'static str;
    async fn connect(&self, profile: &ConnectionProfile) -> OcResult<Box<dyn ProtocolSession>>;
}

#[async_trait]
pub trait ProtocolSession: Send {
    fn kind(&self) -> ProtocolKind;
    fn peer(&self) -> String;

    async fn list_dir(&mut self, path: &str) -> OcResult<Vec<RemoteEntry>>;
    async fn upload_file(&mut self, local_path: &str, remote_path: &str) -> OcResult<()>;
    async fn download_file(&mut self, remote_path: &str, local_path: &str) -> OcResult<()>;
    async fn delete_path(&mut self, remote_path: &str) -> OcResult<()>;
    async fn rename_path(&mut self, from: &str, to: &str) -> OcResult<()>;
    async fn disconnect(&mut self) -> OcResult<()>;
}
