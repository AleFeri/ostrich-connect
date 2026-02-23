#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <protocol-name>"
  exit 1
fi

name="$1"
if [[ ! "$name" =~ ^[a-z0-9-]+$ ]]; then
  echo "protocol name must match ^[a-z0-9-]+$"
  exit 1
fi

crate="oc-protocol-${name}"
dir="crates/${crate}"
type_name=""
IFS='-' read -r -a name_parts <<< "$name"
for part in "${name_parts[@]}"; do
  first="${part:0:1}"
  rest="${part:1}"
  type_name+="${first^^}${rest}"
done

if [[ -e "$dir" ]]; then
  echo "${dir} already exists"
  exit 1
fi

mkdir -p "$dir/src"

cat > "$dir/Cargo.toml" <<EOF2
[package]
name = "${crate}"
version = "0.1.0"
edition = "2021"

[dependencies]
async-trait = "0.1"
oc-core = { path = "../oc-core" }
EOF2

cat > "$dir/src/lib.rs" <<EOF2
use async_trait::async_trait;
use oc_core::error::{OcError, OcResult};
use oc_core::protocol::{ProtocolFactory, ProtocolSession};
use oc_core::types::{ConnectionProfile, ProtocolKind, RemoteEntry};

#[derive(Default)]
pub struct ${type_name}ProtocolFactory;

impl ${type_name}ProtocolFactory {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProtocolFactory for ${type_name}ProtocolFactory {
    fn kind(&self) -> ProtocolKind {
        ProtocolKind::Ftp
    }

    fn display_name(&self) -> &'static str {
        "${type_name}"
    }

    async fn connect(&self, _profile: &ConnectionProfile) -> OcResult<Box<dyn ProtocolSession>> {
        Err(OcError::OperationNotSupported(
            "replace scaffolded kind and session implementation".to_owned(),
        ))
    }
}

pub struct ${type_name}Session;

#[async_trait]
impl ProtocolSession for ${type_name}Session {
    fn kind(&self) -> ProtocolKind {
        ProtocolKind::Ftp
    }

    fn peer(&self) -> String {
        "".to_owned()
    }

    async fn list_dir(&mut self, _path: &str) -> OcResult<Vec<RemoteEntry>> {
        Err(OcError::OperationNotSupported("not implemented".to_owned()))
    }

    async fn upload_file(&mut self, _local_path: &str, _remote_path: &str) -> OcResult<()> {
        Err(OcError::OperationNotSupported("not implemented".to_owned()))
    }

    async fn download_file(&mut self, _remote_path: &str, _local_path: &str) -> OcResult<()> {
        Err(OcError::OperationNotSupported("not implemented".to_owned()))
    }

    async fn delete_path(&mut self, _remote_path: &str) -> OcResult<()> {
        Err(OcError::OperationNotSupported("not implemented".to_owned()))
    }

    async fn rename_path(&mut self, _from: &str, _to: &str) -> OcResult<()> {
        Err(OcError::OperationNotSupported("not implemented".to_owned()))
    }

    async fn disconnect(&mut self) -> OcResult<()> {
        Ok(())
    }
}
EOF2

echo "Created ${dir}"
echo "Next:"
echo "1) Replace ProtocolKind in ${dir}/src/lib.rs"
echo "2) Implement connect/list/upload/download/delete/rename"
echo "3) Register factory where backend is built"
