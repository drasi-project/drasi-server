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

import { useEffect, useState, useRef } from 'react';
import { useSSEClient } from './useSSEClient';
import { QueryResult, QueryOptions, QueryState } from '../core/types';
import { transformSnakeToCamel, parseNumericStrings } from '../utils/transforms';

const defaultGetItemKey = (item: any): string => {
  return item.id || item._id || JSON.stringify(item);
};

/**
 * Hook to subscribe to a Drasi query with real-time updates
 */
export function useQuery<T = any>(
  queryId: string,
  options: QueryOptions<any, T> = {}
): QueryState<T> {
  const [data, setData] = useState<T[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdate, setLastUpdate] = useState<Date | null>(null);
  const sse = useSSEClient();
  const unsubscribeRef = useRef<(() => void) | null>(null);
  const dataMapRef = useRef<Map<string, T>>(new Map());

  const {
    transform,
    accumulationStrategy = 'merge',
    sortBy,
    filter,
    getItemKey = defaultGetItemKey,
  } = options;

  useEffect(() => {
    if (!sse) {
      return;
    }

    setLoading(true);
    setError(null);

    const handleResult = (result: QueryResult) => {
      try {
        let processedData = result.data.map((item: any) => {
          let processed = transformSnakeToCamel(item);
          processed = parseNumericStrings(processed);
          if (transform) {
            processed = transform(processed);
          }
          return processed as T;
        });

        if (filter) {
          processedData = processedData.filter(filter);
        }

        processedData.forEach(item => {
          const key = getItemKey(item);
          dataMapRef.current.set(key, item);
        });

        let finalData = Array.from(dataMapRef.current.values());

        if (sortBy) {
          finalData.sort(sortBy);
        }

        setData(finalData);
        setLastUpdate(new Date(result.timestamp));
        setLoading(false);
      } catch (err) {
        setError(String(err));
        setLoading(false);
      }
    };

    unsubscribeRef.current = sse.subscribe(queryId, handleResult);

    return () => {
      if (unsubscribeRef.current) {
        unsubscribeRef.current();
        unsubscribeRef.current = null;
      }
      dataMapRef.current.clear();
    };
  }, [queryId, sse, transform, accumulationStrategy, sortBy, filter, getItemKey]);

  return { data, loading, error, lastUpdate };
}
