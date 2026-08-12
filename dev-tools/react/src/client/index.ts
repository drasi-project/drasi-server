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

/**
 * Framework-agnostic core: orchestrates the query/reaction lifecycle and the
 * shared, multiplexed SSE connection without any React dependency.
 */

export { DrasiClient } from './DrasiClient';
export type { DrasiClientOptions } from './DrasiClient';
export { DrasiSSEClient } from './DrasiSSEClient';
export type {
  DrasiSSEClientOptions,
  EventSourceFactory,
  EventSourceLike,
} from './DrasiSSEClient';
