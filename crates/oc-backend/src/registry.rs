use std::collections::HashMap;
use std::sync::Arc;

use oc_core::protocol::ProtocolFactory;
use oc_core::types::ProtocolKind;

#[derive(Default)]
pub struct ProtocolRegistry {
    factories: HashMap<ProtocolKind, Arc<dyn ProtocolFactory>>,
}

impl ProtocolRegistry {
    pub fn register<T>(&mut self, factory: T)
    where
        T: ProtocolFactory + 'static,
    {
        self.factories.insert(factory.kind(), Arc::new(factory));
    }

    pub fn get(&self, protocol: ProtocolKind) -> Option<Arc<dyn ProtocolFactory>> {
        self.factories.get(&protocol).cloned()
    }

    pub fn supported(&self) -> Vec<ProtocolKind> {
        let mut protocols = self.factories.keys().copied().collect::<Vec<_>>();
        protocols.sort_by_key(|kind| match kind {
            ProtocolKind::Ftp => 0,
            ProtocolKind::Ftps => 1,
            ProtocolKind::Sftp => 2,
        });
        protocols
    }
}
