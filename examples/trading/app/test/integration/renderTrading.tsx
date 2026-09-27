// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { render, screen, within } from '@testing-library/react';
import { type EventSourceLike } from '@drasi/react';
import { vi } from 'vitest';
import App from '../../src/App';
import { TradingProvider } from '../../src/drasi/TradingProvider';
import { SyntheticTrading } from '../fixtures/synthetic/trading';

class SyntheticEventSource extends EventTarget implements EventSourceLike {
  onopen: EventSourceLike['onopen'] = null;
  onmessage: EventSourceLike['onmessage'] = null;
  onerror: EventSourceLike['onerror'] = null;
  closed = false;

  close(): void { this.closed = true; }
  fail(): void { this.onerror?.(new Event('error')); }
}

const reconnect = { initialReconnectDelayMs: 25, maxReconnectDelayMs: 25 };

export async function renderTrading(backend = new SyntheticTrading()) {
  const sources: SyntheticEventSource[] = [];
  const fetcher: typeof fetch = async (input, init) => {
    if (init?.signal?.aborted) throw new DOMException('Aborted', 'AbortError');
    const url = new URL(input instanceof Request ? input.url : String(input));
    const result = backend.handle({
      path: url.pathname,
      method: init?.method ?? 'GET',
      body: typeof init?.body === 'string' ? JSON.parse(init.body) : undefined,
    });
    return new Response(JSON.stringify(result.body), {
      status: result.status,
      headers: { 'Content-Type': 'application/json' },
    });
  };
  vi.stubGlobal('fetch', fetcher);
  backend.onBatch = batch => {
    for (const source of sources) {
      if (!source.closed) source.onmessage?.(new MessageEvent('message', { data: JSON.stringify(batch) }));
    }
  };
  const rendered = render(
    <TradingProvider
      fetch={fetcher}
      eventSourceFactory={() => {
        const source = new SyntheticEventSource();
        sources.push(source);
        queueMicrotask(() => {
          if (!source.closed) source.onopen?.(new Event('open'));
        });
        return source;
      }}
      reconnect={reconnect}
    >
      <App />
    </TradingProvider>,
  );
  await screen.findByText('Connected');
  await screen.findByRole('button', { name: 'Add to watchlist' });
  await screen.findByText('Total Value');
  return { ...rendered, backend, sources };
}

export function panel(title: string): HTMLElement {
  const element = screen.getByRole('heading', { name: title })
    .closest<HTMLElement>('.drasi-query-table');
  if (!element) throw new Error(`No table card for ${title}`);
  return element;
}

export function symbols(title: string): string[] {
  return within(panel(title)).getAllByRole('row').slice(1)
    .map(row => within(row).getAllByRole('cell')[0].textContent ?? '');
}

export function row(title: string, symbol: string): HTMLElement {
  return within(panel(title)).getByRole('row', { name: new RegExp(`^${symbol} `) });
}

export function summary(label: string): string | null | undefined {
  return within(panel('Portfolio')).getByText(label).nextElementSibling?.textContent;
}
