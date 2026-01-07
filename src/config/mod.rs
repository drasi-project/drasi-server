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

//! Configuration management for Drasi Server.
//!
//! This module provides comprehensive configuration handling including:
//! - Type-safe configuration structures
//! - YAML and JSON file loading
//! - Configuration validation
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```no_run
//! use drasi_server::config;
//!
//! // Load configuration from file (auto-detects YAML/JSON)
//! let config = config::load_config_file("config.yaml").unwrap();
//!
//! println!("Server configuration loaded successfully");
//! ```

pub mod loader;
pub mod types;

// Re-export commonly used types
pub use loader::{from_json_str, from_yaml_str, load_config_file, save_config_file, ConfigError};
pub use types::{DrasiLibInstanceConfig, DrasiServerConfig, ResolvedInstanceConfig};

// Re-export config enums from api::models for backward compatibility
pub use crate::api::models::{ReactionConfig, SourceConfig, StateStoreConfig};
