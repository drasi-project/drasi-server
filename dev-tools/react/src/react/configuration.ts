// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import type { DrasiClientOptions } from '../client/DrasiClient';
import { normalizeServerUrl, validateHttpUrl } from '../client/resources';

/** Compare data by value, callable transports/auth/routing separately by identity. Never log this key. */
export function configurationKey(options: DrasiClientOptions): string | null {
  try {
    const { reconnect } = options;
    return JSON.stringify([
      normalizeServerUrl(options.serverUrl, {}), options.instanceId,
      [...options.queryIds].sort(), options.reaction.id,
      validateHttpUrl(options.reaction.endpoint, {}),
      options.credentials ?? 'same-origin',
      typeof options.headers === 'function' ? null : [...new Headers(options.headers).entries()],
      options.requestTimeoutMs ?? 10000,
      reconnect?.maxReconnectAttempts ?? 10,
      reconnect?.initialReconnectDelayMs ?? 1000,
      reconnect?.maxReconnectDelayMs ?? 30000,
      reconnect?.connectionTimeoutMs ?? 10000,
      options.reconciliation?.maxPendingChanges ?? 10000,
    ]);
  } catch {
    // Invalid configuration is surfaced by the provider, not thrown during render.
    return null;
  }
}
