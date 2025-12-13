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
 * Transform snake_case keys to camelCase
 */
export function transformSnakeToCamel(obj: any): any {
  if (obj === null || obj === undefined) {
    return obj;
  }

  if (Array.isArray(obj)) {
    return obj.map(transformSnakeToCamel);
  }

  if (typeof obj === 'object') {
    const transformed: any = {};
    for (const [key, value] of Object.entries(obj)) {
      const camelKey = key.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
      transformed[camelKey] = transformSnakeToCamel(value);
    }
    return transformed;
  }

  return obj;
}

/**
 * Parse numeric strings to numbers
 */
export function parseNumericStrings(obj: any): any {
  if (obj === null || obj === undefined) {
    return obj;
  }

  if (Array.isArray(obj)) {
    return obj.map(parseNumericStrings);
  }

  if (typeof obj === 'object') {
    const parsed: any = {};
    for (const [key, value] of Object.entries(obj)) {
      if (typeof value === 'string' && value !== '') {
        const num = parseFloat(value);
        parsed[key] = isNaN(num) ? value : num;
      } else {
        parsed[key] = parseNumericStrings(value);
      }
    }
    return parsed;
  }

  return obj;
}

/**
 * Deep clone an object
 */
export function deepClone<T>(obj: T): T {
  if (obj === null || typeof obj !== 'object') {
    return obj;
  }

  if (Array.isArray(obj)) {
    return obj.map(deepClone) as any;
  }

  const cloned: any = {};
  for (const [key, value] of Object.entries(obj)) {
    cloned[key] = deepClone(value);
  }
  return cloned;
}
