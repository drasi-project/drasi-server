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

//! Reaction configuration DTOs

pub mod grpc_reaction;
pub mod http_reaction;
pub mod log;
pub mod platform_reaction;
pub mod profiler;
pub mod sse;

pub use grpc_reaction::*;
pub use http_reaction::*;
// Note: log and sse modules have types with similar names (QueryConfigDto, TemplateSpecDto)
// They should be accessed via their module namespaces: log::*, sse::*
pub use log::LogReactionConfigDto;
pub use platform_reaction::*;
pub use profiler::*;
pub use sse::SseReactionConfigDto;
