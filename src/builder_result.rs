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

use drasi_server_core::{DrasiServerCore, ApplicationHandle};
use std::collections::HashMap;
use std::sync::Arc;

/// Result of building a DrasiServer with application integration
pub struct DrasiServerWithHandles {
    /// The server core for controlling the server
    pub server: Arc<DrasiServerCore>,
    /// Map of application handles by name
    pub handles: HashMap<String, ApplicationHandle>,
}