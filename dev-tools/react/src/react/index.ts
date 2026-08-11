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
 * React bindings: the provider that owns the shared connection and the hooks
 * that expose each query's live result set as React state.
 */

export {
  DrasiProvider,
  useDrasiClient,
  useDrasiQuery,
  useDrasiConnectionStatus,
  useDrasiServerUiUrl,
  useDrasiQueryDefinition,
} from './DrasiContext';
export type { DrasiProviderProps } from './DrasiContext';
export { useRowAnimation } from './useRowAnimation';
export type {
  AnimationDirection,
  UseRowAnimationOptions,
  UseRowAnimationResult,
} from './useRowAnimation';
