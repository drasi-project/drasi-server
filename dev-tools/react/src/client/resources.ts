// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import type {
  Component, ComponentStatus, JsonValue, QueryConfig, QueryJoin, QueryMiddleware,
  QueryInput, ReactionConfig, ResultRow,
} from './types';
import { DrasiError, asDrasiError, type DrasiErrorDetails } from './errors';

export function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every(item => typeof item === 'string');
}

function isJsonValue(value: unknown): value is JsonValue {
  return value === null || typeof value === 'string' || typeof value === 'boolean' ||
    (typeof value === 'number' && Number.isFinite(value)) ||
    (Array.isArray(value) && value.every(isJsonValue)) ||
    (isRecord(value) && Object.values(value).every(isJsonValue));
}

function isJsonObject(value: unknown): value is Record<string, JsonValue> {
  return isRecord(value) && Object.values(value).every(isJsonValue);
}

function isUnsignedInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

/** The raw row boundary is an object, not an assertion about application fields. */
export function readRows(value: unknown, details: DrasiErrorDetails): ResultRow[] {
  if (!Array.isArray(value) || !value.every(isJsonObject)) {
    throw new DrasiError('INVALID_PAYLOAD', details);
  }
  return value;
}

export function isIdentifier(value: unknown): value is string {
  if (typeof value !== 'string' || !value.trim() || value === '.' || value === '..') return false;
  try {
    encodeURIComponent(value);
    return true;
  } catch {
    return false;
  }
}

/** Validate and serialize without trimming meaningful path or query content. */
export function validateHttpUrl(value: string, details: DrasiErrorDetails): string {
  try {
    const url = new URL(value);
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password ||
        url.hash || ['0.0.0.0', '[::]'].includes(url.hostname)) {
      throw new DrasiError('INVALID_CONFIGURATION', details);
    }
    return url.toString();
  } catch {
    throw new DrasiError('INVALID_CONFIGURATION', details);
  }
}

/** Normalize only the base used to append API/UI paths, never a stream endpoint. */
export function normalizeServerUrl(value: string, details: DrasiErrorDetails): string {
  const url = validateHttpUrl(value, details);
  if (new URL(url).search) throw new DrasiError('INVALID_CONFIGURATION', details);
  return url.replace(/\/$/, '');
}

/** Never use convenience routes: every read has the same explicit instance. */
export function instancePath(base: string, instanceId: string, ...segments: string[]): string {
  // URL parsers normalize dot segments even when percent-encoded. Reject them
  // rather than accidentally addressing another instance/convenience route.
  if (!isIdentifier(instanceId) || segments.some(segment => !isIdentifier(segment))) {
    throw new DrasiError('INVALID_CONFIGURATION', { instanceId });
  }
  return `${base}/api/v1/instances/${encodeURIComponent(instanceId)}/${segments.map(encodeURIComponent).join('/')}`;
}

const statuses: readonly ComponentStatus[] = [
  'Starting', 'Running', 'Stopping', 'Stopped', 'Error', 'Reconfiguring', 'Added', 'Removed',
];

export function readComponent(
  value: unknown,
  details: DrasiErrorDetails,
): Component<Record<string, unknown>> {
  if (!isRecord(value) || !isIdentifier(value.id) ||
      (value.name !== undefined && !isIdentifier(value.name)) ||
      (value.id !== details.resourceId && value.name !== details.resourceId) ||
      !isRecord(value.config) || !isRecord(value.links) ||
      (value.error_message !== undefined && typeof value.error_message !== 'string')) {
    throw new DrasiError('INVALID_PAYLOAD', details);
  }
  const status = statuses.find(status => status === value.status);
  if (!status) throw new DrasiError('INVALID_PAYLOAD', details);
  const kind = details.resourceKind === 'query' ? 'queries' : 'reactions';
  // Links address the component by the identifier used in the request (UUID
  // or name). Accept literal or encoded segments, but expose only an
  // unambiguous, instance-scoped link.
  const identifiers = [value.id, value.name, details.resourceId].filter(isIdentifier);
  const encoded = identifiers.map(id => instancePath('', details.instanceId ?? '', kind, id));
  const links = [...identifiers.map(id => `/api/v1/instances/${details.instanceId}/${kind}/${id}`), ...encoded];
  const index = links.indexOf(value.links.self as string);
  if (index < 0 || value.links.full !== `${value.links.self}?view=full`) {
    throw new DrasiError('INVALID_PAYLOAD', details);
  }
  const self = encoded[index % identifiers.length];
  return {
    id: value.id, ...(value.name === undefined ? {} : { name: value.name }), status, config: value.config,
    links: { self, full: `${self}?view=full` },
    ...(value.error_message === undefined ? {} : { error_message: value.error_message }),
  };
}

function isOptionalStringArray(value: unknown): boolean {
  return value === undefined || isStringArray(value);
}

function isInput(value: unknown): value is QueryInput {
  if (isIdentifier(value)) return true;
  return isRecord(value) &&
    (isIdentifier(value.component) ? value.id === undefined : isIdentifier(value.id)) &&
    (value.port === undefined || isIdentifier(value.port)) &&
    isOptionalStringArray(value.nodes) && isOptionalStringArray(value.relations) &&
    isOptionalStringArray(value.pipeline) &&
    Object.keys(value).every(key => ['component', 'id', 'port', 'nodes', 'relations', 'pipeline'].includes(key));
}

function isResult(value: unknown): boolean {
  return value === undefined || (isRecord(value) &&
    (value.label === undefined || (typeof value.label === 'string' && value.label.length > 0)));
}

function isJoin(value: unknown): value is QueryJoin {
  return isRecord(value) && isIdentifier(value.id) && Array.isArray(value.keys) &&
    value.keys.every(key => isRecord(key) && typeof key.label === 'string' && typeof key.property === 'string');
}

function isMiddleware(value: unknown): value is QueryMiddleware {
  return isRecord(value) && typeof value.kind === 'string' && typeof value.name === 'string' &&
    isJsonObject(value.config);
}

function isQueryConfig(value: unknown): value is QueryConfig {
  return isRecord(value) && isIdentifier(value.id) && typeof value.query === 'string' &&
    (value.queryLanguage === 'Cypher' || value.queryLanguage === 'GQL') &&
    typeof value.autoStart === 'boolean' && typeof value.enableBootstrap === 'boolean' &&
    isUnsignedInteger(value.outboxCapacity) &&
    isUnsignedInteger(value.bootstrapTimeoutSecs) &&
    Array.isArray(value.middleware) && value.middleware.every(isMiddleware) &&
    Array.isArray(value.inputs) && value.inputs.every(isInput) && isResult(value.result) &&
    (value.joins === undefined || (Array.isArray(value.joins) && value.joins.every(isJoin))) &&
    (value.priorityQueueCapacity === undefined || isUnsignedInteger(value.priorityQueueCapacity)) &&
    (value.dispatchBufferCapacity === undefined || isUnsignedInteger(value.dispatchBufferCapacity)) &&
    (value.dispatchMode === undefined || value.dispatchMode === 'Channel') &&
    (value.storageBackend === undefined || isJsonValue(value.storageBackend));
}

export function readQuery(value: unknown, details: DrasiErrorDetails): Component<QueryConfig> {
  const component = readComponent(value, details);
  const config = component.config;
  if (!isQueryConfig(config) || config.id !== component.id) {
    throw new DrasiError('INVALID_PAYLOAD', details);
  }
  return { ...component, config };
}

function isReactionConfig(value: unknown): value is ReactionConfig {
  return isJsonObject(value) && isIdentifier(value.id) && typeof value.kind === 'string' &&
    isStringArray(value.queries) && value.queries.every(isIdentifier) &&
    (value.autoStart === undefined || typeof value.autoStart === 'boolean') &&
    (value.identityProvider === undefined || isIdentifier(value.identityProvider)) &&
    (value.kind !== 'sse' || (
      typeof value.host === 'string' &&
      isUnsignedInteger(value.port) && value.port > 0 && value.port <= 65535 &&
      typeof value.ssePath === 'string' && value.ssePath.startsWith('/') &&
      isUnsignedInteger(value.heartbeatIntervalMs)
    ));
}

// Servers report subscribed queries by name or as `{ id }` references to the
// query component's UUID. Expose them uniformly as identifiers.
function reactionQuery(value: unknown): unknown {
  return isRecord(value) && Object.keys(value).length === 1 && isIdentifier(value.id) ? value.id : value;
}

export function readReaction(value: unknown, details: DrasiErrorDetails): Component<ReactionConfig> {
  const component = readComponent(value, details);
  const raw = component.config;
  const config = Array.isArray(raw.queries) ? { ...raw, queries: raw.queries.map(reactionQuery) } : raw;
  if (!isReactionConfig(config) ||
      (config.id !== component.id && (component.name === undefined || config.id !== component.name))) {
    throw new DrasiError('INVALID_PAYLOAD', details);
  }
  return { ...component, config };
}

export function requireRunning(component: Component<unknown>, details: DrasiErrorDetails): void {
  if (component.status === 'Running') return;
  const code = component.status === 'Starting' || component.status === 'Reconfiguring'
    ? 'RESOURCE_STARTING'
    : component.status === 'Stopped' || component.status === 'Added'
      ? 'RESOURCE_STOPPED'
      : 'RESOURCE_UNAVAILABLE';
  throw new DrasiError(code, { ...details, resourceStatus: component.status });
}

async function readJson(response: Response, details: DrasiErrorDetails): Promise<unknown> {
  try {
    return await response.json();
  } catch (error) {
    throw asDrasiError(error, details, error instanceof SyntaxError ? 'INVALID_PAYLOAD' : 'SERVER_UNAVAILABLE');
  }
}

export async function readResponse(response: Response, details: DrasiErrorDetails): Promise<unknown> {
  const errorDetails = { ...details, status: response.status };
  if (!response.ok) {
    if (response.status === 401) throw new DrasiError('UNAUTHENTICATED', errorDetails);
    if (response.status === 403) throw new DrasiError('FORBIDDEN', errorDetails);
    if (response.status === 404) {
      // Only a structured REST resource error establishes absence. HTML 404s
      // (for example a wrong proxy URL) must never authorize app provisioning.
      const body = await readJson(response, errorDetails);
      const code = isRecord(body) ? body.code : null;
      if (code === 'INSTANCE_NOT_FOUND') {
        throw new DrasiError(code, {
          ...errorDetails, resourceKind: 'instance', resourceId: details.instanceId,
        });
      }
      if ((code === 'QUERY_NOT_FOUND' && details.resourceKind === 'query') ||
          (code === 'REACTION_NOT_FOUND' && details.resourceKind === 'reaction')) {
        throw new DrasiError(code, errorDetails);
      }
      throw new DrasiError('INVALID_PAYLOAD', errorDetails);
    }
    throw new DrasiError(
      response.status >= 500 || response.status === 408 || response.status === 429
        ? 'SERVER_UNAVAILABLE' : 'INCOMPATIBLE_RESOURCE',
      errorDetails,
    );
  }
  const body = await readJson(response, errorDetails);
  if (!isRecord(body) || body.success !== true || !('data' in body) ||
      (body.error !== undefined && body.error !== null)) {
    throw new DrasiError('INVALID_PAYLOAD', errorDetails);
  }
  return body.data;
}
