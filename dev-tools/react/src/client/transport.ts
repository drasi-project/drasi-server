// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { DrasiError, type DrasiErrorDetails } from './errors';
import type { DrasiHeaders, DrasiRequestContext } from './types';

export function validateCredentials(
  credentials: RequestCredentials = 'same-origin',
  details: DrasiErrorDetails = {},
): RequestCredentials {
  if (!['omit', 'same-origin', 'include'].includes(credentials)) {
    throw new DrasiError('INVALID_CONFIGURATION', details);
  }
  return credentials;
}

export function requestHeaders(
  value: HeadersInit | undefined,
  transport: 'rest' | 'sse' | undefined,
  details: DrasiErrorDetails,
): Headers {
  try {
    const headers = new Headers(value);
    if (transport !== undefined && !headers.has('Accept')) {
      headers.set('Accept', transport === 'rest' ? 'application/json' : 'text/event-stream');
    }
    return headers;
  } catch {
    throw new DrasiError('INVALID_CONFIGURATION', details);
  }
}

export async function resolveHeaders(
  headers: DrasiHeaders | undefined,
  context: DrasiRequestContext,
  details: DrasiErrorDetails,
): Promise<Headers> {
  const value = typeof headers === 'function' ? await headers(context) : headers;
  context.signal.throwIfAborted();
  if (typeof headers === 'function' && value === undefined) {
    throw new DrasiError('INVALID_CONFIGURATION', details);
  }
  return requestHeaders(value, context.transport, details);
}

/** Bound even a custom transport/auth callback that fails to settle after cancellation. */
export function abortable<T>(work: Promise<T>, signal: AbortSignal): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const abort = () => reject(signal.reason);
    signal.addEventListener('abort', abort, { once: true });
    if (signal.aborted) abort();
    void work.then(resolve, reject).finally(() => signal.removeEventListener('abort', abort));
  });
}
