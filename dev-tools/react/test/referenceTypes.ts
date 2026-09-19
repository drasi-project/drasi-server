// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import type { DrasiClientOptions, DrasiError, DrasiProviderProps, ReactionReference } from '../src';

const reaction: ReactionReference = { id: 'events', endpoint: 'https://events.example/changes' };
const options: DrasiClientOptions = {
  serverUrl: 'https://drasi.example', instanceId: 'analytics', queryIds: ['readings'], reaction,
};
export const provider: DrasiProviderProps = { ...options, children: null };

// @ts-expect-error Instance identity must be explicitly resolved by the consumer.
export const missingInstance: DrasiClientOptions = { serverUrl: options.serverUrl, queryIds: ['readings'], reaction };
// @ts-expect-error Connection options do not accept deployment definitions.
export const deployment: DrasiClientOptions = { ...options, queries: [{ id: 'readings', query: 'MATCH (n) RETURN n', sources: [] }] };
// @ts-expect-error A server bind port is not a browser endpoint reference.
export const bindSettings: ReactionReference = { id: 'events', port: 8081 };

export function handleError(error: DrasiError): string {
  const retryable: boolean = error.retryable;
  return error.code === 'QUERY_NOT_FOUND' && !retryable ? error.resourceId ?? '' : error.message;
}
