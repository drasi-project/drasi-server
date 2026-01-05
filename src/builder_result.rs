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
