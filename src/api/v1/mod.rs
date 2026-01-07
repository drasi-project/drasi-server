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

//! API Version 1 (v1) implementation.
//!
//! This module contains the v1 API handlers, routes, and OpenAPI documentation.
//! All v1 endpoints are accessible under the `/api/v1/` prefix.
//!
//! ## Endpoint Structure
//!
//! - `GET /api/v1/instances` - List all DrasiLib instances
//! - `GET /api/v1/instances/{instanceId}/sources` - List sources for an instance
//! - `POST /api/v1/instances/{instanceId}/sources` - Create a source
//! - `GET /api/v1/instances/{instanceId}/sources/{id}` - Get source status
//! - `DELETE /api/v1/instances/{instanceId}/sources/{id}` - Delete a source
//! - `POST /api/v1/instances/{instanceId}/sources/{id}/start` - Start a source
//! - `POST /api/v1/instances/{instanceId}/sources/{id}/stop` - Stop a source
//! - `GET /api/v1/instances/{instanceId}/queries` - List queries
//! - `POST /api/v1/instances/{instanceId}/queries` - Create a query
//! - `GET /api/v1/instances/{instanceId}/queries/{id}` - Get query config
//! - `DELETE /api/v1/instances/{instanceId}/queries/{id}` - Delete a query
//! - `POST /api/v1/instances/{instanceId}/queries/{id}/start` - Start a query
//! - `POST /api/v1/instances/{instanceId}/queries/{id}/stop` - Stop a query
//! - `GET /api/v1/instances/{instanceId}/queries/{id}/results` - Get query results
//! - `GET /api/v1/instances/{instanceId}/reactions` - List reactions
//! - `POST /api/v1/instances/{instanceId}/reactions` - Create a reaction
//! - `GET /api/v1/instances/{instanceId}/reactions/{id}` - Get reaction status
//! - `DELETE /api/v1/instances/{instanceId}/reactions/{id}` - Delete a reaction
//! - `POST /api/v1/instances/{instanceId}/reactions/{id}/start` - Start a reaction
//! - `POST /api/v1/instances/{instanceId}/reactions/{id}/stop` - Stop a reaction
//!
//! ## Convenience Routes (First Instance)
//!
//! For backward compatibility and convenience, the first configured instance
//! is also accessible via shortened routes:
//!
//! - `/api/v1/sources` - Sources of the first instance
//! - `/api/v1/queries` - Queries of the first instance
//! - `/api/v1/reactions` - Reactions of the first instance

pub mod handlers;
pub mod openapi;
pub mod routes;

pub use handlers::*;
pub use openapi::ApiDocV1;
pub use routes::build_v1_router;
