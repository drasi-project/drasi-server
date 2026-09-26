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
 * `@drasi/react` — reusable React building blocks for UIs driven by Drasi
 * Continuous Queries.
 *
 * - {@link DrasiProvider}: opens a single shared SSE connection to a Drasi
 *   Server's SSE Reaction and multiplexes every Continuous Query over it.
 * - {@link useDrasiQuery} and friends: subscribe to a query's live result set.
 * - {@link QueryTable}: a sortable, animated table bound to a query.
 *
 * The public API is organized into three groups:
 * - `client`     — framework-agnostic core (`DrasiClient`, `DrasiSSEClient`).
 * - `react`      — React bindings (`DrasiProvider`, hooks).
 * - `components` — ready-made UI (`QueryTable`, `CodeViewerDialog`, icons).
 */

export * from './client';
export * from './react';
export * from './components';
export type * from './types';
