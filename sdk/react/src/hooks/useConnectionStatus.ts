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

import { useEffect, useState } from 'react';
import { useSSEClient } from './useSSEClient';
import { ConnectionStatus } from '../core/types';

/**
 * Hook to monitor Drasi SSE connection status
 */
export function useConnectionStatus(): ConnectionStatus {
  const [status, setStatus] = useState<ConnectionStatus>({ connected: false });
  const sse = useSSEClient();

  useEffect(() => {
    if (!sse) {
      return;
    }

    const checkStatus = () => {
      setStatus(sse.getConnectionStatus());
    };

    checkStatus();
    const interval = setInterval(checkStatus, 5000);

    return () => clearInterval(interval);
  }, [sse]);

  return status;
}
