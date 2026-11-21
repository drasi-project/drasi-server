// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use drasi_lib::DrasiLib;
use std::collections::HashMap;
use std::sync::Arc;

/// Result of building a DrasiServer
///
/// Note: Application source/reaction handles were removed during the plugin architecture refactor.
/// Use the builder pattern in drasi-lib directly for programmatic integration.
pub struct DrasiServerWithHandles {
    /// Primary server core for backward compatibility (first configured instance)
    pub server: Arc<DrasiLib>,
    /// The server cores keyed by DrasiLib instance id
    pub servers: HashMap<String, Arc<DrasiLib>>,
}

/// Started Core handles retaining Server's instance-cleanup owner.
///
/// Borrowed telemetry providers remain the caller's responsibility. Dropping
/// this result requests cleanup; keep its cleanup handle and a live Tokio
/// runtime until shutdown is confirmed.
pub struct DrasiServerWithOwnedHandles {
    pub server: Arc<DrasiLib>,
    pub servers: HashMap<String, Arc<DrasiLib>>,
    pub(crate) cleanup: crate::server::ServerCleanupHandle,
}

impl DrasiServerWithOwnedHandles {
    pub fn cleanup_handle(&self) -> crate::server::ServerCleanupHandle {
        self.cleanup.clone()
    }

    /// Join the same cleanup attempt, without replenishing its original budget.
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        self.cleanup.shutdown().await
    }
}

impl Drop for DrasiServerWithOwnedHandles {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup.request_shutdown() {
            log::error!("Server cleanup remains owned but could not be scheduled: {error}");
        }
    }
}
