//! Network Namespace Isolation Manager.

use gaxera_abi::GaxObjectId;
use net_types::NetRoute;

pub struct NamespaceEntry {
    pub id: GaxObjectId,
    pub routes: [Option<NetRoute>; 16],
}

pub struct NetNamespaceManager {
    namespaces: [Option<NamespaceEntry>; 16],
}

impl Default for NetNamespaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl NetNamespaceManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            namespaces: [const { None }; 16],
        };
        // Root Namespace
        mgr.namespaces[0] = Some(NamespaceEntry {
            id: GaxObjectId::generate(),
            routes: [const { None }; 16],
        });
        mgr
    }

    pub fn create_namespace(&mut self) -> Result<GaxObjectId, net_types::ProviderError> {
        let id = GaxObjectId::generate();
        for slot in self.namespaces.iter_mut() {
            if slot.is_none() {
                *slot = Some(NamespaceEntry {
                    id,
                    routes: [const { None }; 16],
                });
                return Ok(id);
            }
        }
        Err(net_types::ProviderError::NotReady)
    }
}
