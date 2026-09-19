// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { useState } from 'react';
import { useDrasiClient, useDrasiQueryDefinition, useDrasiServerUiUrl } from '@drasi/react/react';
import type { QueryConfig } from '@drasi/react/client';
import { CodeViewerDialog } from './CodeViewerDialog';

/** Trading's displayed query-definition format; not part of table presentation. */
export function formatQueryConfig(config: QueryConfig): string {
  const lines: string[] = [];
  const addField = (label: string, value: unknown) => {
    if (value === undefined || value === null) return;
    if (typeof value === 'string' || typeof value === 'boolean' || typeof value === 'number') {
      lines.push(`${label}: ${value}`);
    } else {
      lines.push(`${label}: ${JSON.stringify(value)}`);
    }
  };
  addField('id', config.id);
  addField('queryLanguage', config.queryLanguage);
  addField('autoStart', config.autoStart);
  if (config.query) {
    const query = config.query.trim();
    lines.push('', 'query: |');
    for (const line of query.split('\n')) lines.push(`  ${line}`);
  }
  if (config.sources.length > 0) {
    lines.push('', 'sources:');
    for (const source of config.sources) {
      lines.push(`  - sourceId: ${source.sourceId}`);
      if (source.pipeline?.length) lines.push(`    pipeline: [${source.pipeline.join(', ')}]`);
      if (source.nodes?.length) lines.push(`    nodes: [${source.nodes.join(', ')}]`);
      if (source.relations?.length) lines.push(`    relations: [${source.relations.join(', ')}]`);
    }
  }
  if (config.joins) {
    lines.push('', 'joins:');
    for (const join of config.joins) {
      lines.push(`  - id: ${join.id}`);
      if (join.keys.length) {
        lines.push('    keys:');
        for (const key of join.keys) lines.push(`      - label: ${key.label}, property: ${key.property}`);
      }
    }
  }
  addField('enableBootstrap', config.enableBootstrap);
  addField('bootstrapBufferSize', config.bootstrapBufferSize);
  if (config.middleware.length) lines.push(`middleware: [${config.middleware.join(', ')}]`);
  addField('priorityQueueCapacity', config.priorityQueueCapacity);
  addField('dispatchBufferCapacity', config.dispatchBufferCapacity);
  addField('dispatchMode', config.dispatchMode);
  if (config.storageBackend) {
    lines.push('', `storageBackend: ${JSON.stringify(config.storageBackend, null, 2)}`);
  }
  return lines.join('\n');
}

interface QueryInspectorProps {
  queryId: string;
  title: string;
  codeSnippet: string;
  onClose: () => void;
}

function DefinitionAttempt(props: QueryInspectorProps & { onRetry: () => void }) {
  const { config, loading, error } = useDrasiQueryDefinition(props.queryId);
  const { error: connectionError, retry: retryConnection } = useDrasiClient();
  const drasiUiUrl = useDrasiServerUiUrl();
  const sharedFailure = error !== null && error === connectionError;
  const definition = loading ? 'Loading query definition...'
    : error ? `Unable to load query definition: ${error.message}`
    : config ? formatQueryConfig(config) : 'Query not found';
  return (
    <CodeViewerDialog
      isOpen
      onClose={props.onClose}
      title={props.title}
      reactCode={props.codeSnippet}
      cypherQuery={definition}
      drasiUiUrl={drasiUiUrl}
      statusSlot={error && (
        <div role="alert">
          <button type="button" onClick={sharedFailure ? retryConnection : props.onRetry}>
            {sharedFailure ? 'Retry connection' : 'Retry query definition'}
          </button>
        </div>
      )}
    />
  );
}

/** Mount only while open. Local retries remount this read; shared failures use the provider retry. */
export function QueryInspector(props: QueryInspectorProps) {
  const [attempt, setAttempt] = useState(0);
  return <DefinitionAttempt {...props} key={attempt} onRetry={() => setAttempt(attempt + 1)} />;
}
