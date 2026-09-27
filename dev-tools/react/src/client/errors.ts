// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

const messages = {
  INSTANCE_NOT_FOUND: 'The selected Drasi instance does not exist.',
  QUERY_NOT_FOUND: 'The referenced query does not exist.',
  REACTION_NOT_FOUND: 'The referenced reaction does not exist.',
  RESOURCE_STOPPED: 'The referenced resource is stopped.',
  RESOURCE_STARTING: 'The referenced resource is starting or bootstrapping.',
  RESOURCE_UNAVAILABLE: 'The referenced resource is not usable.',
  SERVER_UNAVAILABLE: 'The Drasi server is unavailable. Retry when it is reachable.',
  STREAM_UNAVAILABLE: 'The SSE endpoint is unavailable. Check its URL, access and network.',
  UNAUTHENTICATED: 'Authentication is required to access Drasi.',
  FORBIDDEN: 'Access to the Drasi resource is denied.',
  INVALID_CONFIGURATION: 'The Drasi connection configuration is invalid.',
  INCOMPATIBLE_RESOURCE: 'The resource is incompatible with this connection.',
  INVALID_PAYLOAD: 'Drasi returned an unsupported or malformed payload.',
} as const;

export type DrasiErrorCode = keyof typeof messages;
export type DrasiResourceKind = 'instance' | 'query' | 'reaction';

export interface DrasiErrorDetails {
  instanceId?: string;
  resourceKind?: DrasiResourceKind;
  resourceId?: string;
  /** HTTP status, when REST exposed one. EventSource errors do not expose it. */
  status?: number;
  resourceStatus?: string;
}

/** A safe, actionable failure. Discriminate by code, never by message text. */
export class DrasiError extends Error {
  readonly name = 'DrasiError';
  readonly retryable: boolean;
  readonly instanceId?: string;
  readonly resourceKind?: DrasiResourceKind;
  readonly resourceId?: string;
  readonly status?: number;
  readonly resourceStatus?: string;

  constructor(readonly code: DrasiErrorCode, details: DrasiErrorDetails = {}) {
    super(messages[code]);
    this.retryable = code === 'SERVER_UNAVAILABLE' ||
      code === 'STREAM_UNAVAILABLE' || code === 'RESOURCE_STARTING';
    this.instanceId = details.instanceId;
    this.resourceKind = details.resourceKind;
    this.resourceId = details.resourceId;
    this.status = details.status;
    this.resourceStatus = details.resourceStatus;
  }
}

export function isAbortError(error: unknown): boolean {
  return error instanceof Error && error.name === 'AbortError';
}

export function asDrasiError(
  error: unknown,
  details: DrasiErrorDetails = {},
  code: DrasiErrorCode = 'INVALID_PAYLOAD',
): DrasiError {
  return error instanceof DrasiError ? error : new DrasiError(code, details);
}
