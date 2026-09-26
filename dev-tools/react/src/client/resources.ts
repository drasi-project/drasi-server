// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import type { Component, ComponentStatus, QueryConfig, ReactionConfig } from '../types';
import { DrasiError, asDrasiError, type DrasiErrorDetails } from './errors';

export function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every(item => typeof item === 'string');
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

export function validateHttpUrl(value: string, details: DrasiErrorDetails): string {
  try {
    const url = new URL(value);
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password ||
        url.hash || ['0.0.0.0', '[::]'].includes(url.hostname)) {
      throw new DrasiError('INVALID_CONFIGURATION', details);
    }
    return url.toString().replace(/\/$/, '');
  } catch {
    throw new DrasiError('INVALID_CONFIGURATION', details);
  }
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
  if (!isRecord(value) || typeof value.id !== 'string' || value.id !== details.resourceId ||
      !isRecord(value.config)) {
    throw new DrasiError('INVALID_PAYLOAD', details);
  }
  const status = statuses.find(status => status === value.status);
  if (!status) throw new DrasiError('INVALID_PAYLOAD', details);
  return { id: value.id, status, config: value.config };
}

export function readQuery(value: unknown, details: DrasiErrorDetails): Component<QueryConfig> {
  const component = readComponent(value, details);
  const config = component.config;
  if (config.id !== component.id || typeof config.query !== 'string' ||
      (config.queryLanguage !== 'Cypher' && config.queryLanguage !== 'GQL') ||
      !Array.isArray(config.sources) ||
      !config.sources.every(source => isRecord(source) && typeof source.sourceId === 'string' &&
        ['pipeline', 'nodes', 'relations'].every(key => source[key] === undefined || isStringArray(source[key]))) ||
      (config.joins !== undefined && (!Array.isArray(config.joins) ||
        !config.joins.every(join => isRecord(join) && typeof join.id === 'string' &&
          Array.isArray(join.keys) && join.keys.every(key => isRecord(key) &&
            typeof key.label === 'string' && typeof key.property === 'string'))))) {
    throw new DrasiError('INVALID_PAYLOAD', details);
  }
  // The guard checks the read contract; extra server DTO fields are retained.
  return { ...component, config: config as QueryConfig };
}

export function readReaction(value: unknown, details: DrasiErrorDetails): Component<ReactionConfig> {
  const component = readComponent(value, details);
  const config = component.config;
  if (config.id !== component.id || typeof config.kind !== 'string' || !isStringArray(config.queries)) {
    throw new DrasiError('INVALID_PAYLOAD', details);
  }
  return { ...component, config: config as ReactionConfig };
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
  if (!response.ok) {
    const errorDetails = { ...details, status: response.status };
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
  const body = await readJson(response, details);
  if (!isRecord(body) || body.success !== true || !('data' in body)) {
    throw new DrasiError('INVALID_PAYLOAD', details);
  }
  return body.data;
}
