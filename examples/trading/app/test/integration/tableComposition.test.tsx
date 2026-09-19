// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DrasiClientProvider } from '@drasi/react/react';
import { DrasiError, type QueryConfig } from '@drasi/react/client';
import { CodeViewerDialog } from '../../src/components/CodeViewerDialog';
import { QueryInspector, formatQueryConfig } from '../../src/components/QueryInspector';
import { TradingQueryTable } from '../../src/components/TradingQueryTable';
import { tradingQueryOptions } from '../../src/drasi/queryOptions';
import { SyntheticTrading } from '../fixtures/synthetic/trading';
import { panel, renderTrading } from './renderTrading';

const queryPath = '/api/v1/instances/trading-server/queries/watchlist-query';
function gate() {
  let resolve!: () => void;
  const promise = new Promise<void>(done => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => {
  vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(1);
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {});
});
afterEach(() => vi.useRealTimers());

describe('Trading-owned table composition', () => {
  it('does not inspect any definition while closed, and a delayed read updates the open viewer/copy text', async () => {
    const ready = gate();
    let inspecting = false;
    let inspectionReads = 0;
    const { backend, sources } = await renderTrading(new SyntheticTrading(), async url => {
      if (inspecting && url.pathname === queryPath) {
        inspectionReads++;
        await ready.promise;
      }
    });
    // Required resource checks and baseline reads remain; none is an inspector read.
    const before = backend.requests.length;
    const liveReads = backend.requests.filter(request => request.path === queryPath).length;
    expect(liveReads).toBeGreaterThan(0);
    expect(inspectionReads).toBe(0);
    const user = userEvent.setup();
    inspecting = true;
    await user.click(within(panel('Watchlist')).getByRole('button', { name: 'View code' }));
    const dialog = await screen.findByRole('dialog', { name: 'Watchlist' });
    expect(within(dialog).getByText('Loading query definition...')).not.toBeNull();
    expect(inspectionReads).toBe(1);
    await act(async () => ready.resolve());
    await waitFor(() => expect(dialog.querySelector('code')?.textContent).toContain('ON_WATCHLIST'));
    expect(dialog.querySelector('code')?.textContent).toContain('queryLanguage: Cypher');
    expect(backend.requests.slice(before)).toEqual([{ method: 'GET', path: queryPath, body: undefined }]);
    expect(sources).toHaveLength(1);
    await user.click(within(dialog).getByRole('button', { name: 'Copy' }));
    expect(await navigator.clipboard.readText()).toBe(dialog.querySelector('code')?.textContent);
    expect(within(dialog).getByRole('button', { name: 'Copied!' })).not.toBeNull();
    expect(within(dialog).getByRole('link', { name: 'Open in Drasi UI' }).getAttribute('href'))
      .toBe('http://localhost:8280/ui?instance=trading-server');
    await user.click(within(dialog).getByRole('tab', { name: 'React Code' }));
    expect(dialog.querySelector('code')?.textContent).toContain('<TradingQueryTable<Stock>');
    expect(dialog.querySelector('code')?.textContent).toContain("tradingQueryOptions('watchlist-query')");
    await user.click(within(dialog).getByRole('button', { name: 'Copied!' }));
    expect(await navigator.clipboard.readText()).toBe(dialog.querySelector('code')?.textContent);
    await user.click(within(dialog).getByRole('button', { name: 'Close' }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('reports a safe lazy-read error and retries only the inspector, without disrupting live rows', async () => {
    const { backend, sources } = await renderTrading();
    const normal = panel('Watchlist');
    const before = backend.requests.length;
    backend.failure = { method: 'GET', path: queryPath, status: 503, error: 'private operator details' };
    fireEvent.click(within(panel('Watchlist')).getByRole('button', { name: 'View code' }));
    const dialog = await screen.findByRole('dialog', { name: 'Watchlist' });
    await within(dialog).findByRole('button', { name: 'Retry query definition' });
    expect(dialog.querySelector('code')?.textContent).toContain('Unable to load query definition');
    expect(dialog.textContent).not.toContain('private operator details');
    expect(normal.querySelector('tbody')?.textContent).toContain('$110.00');
    backend.failure = null;
    fireEvent.click(within(dialog).getByRole('button', { name: 'Retry query definition' }));
    await waitFor(() => expect(screen.getByRole('dialog').querySelector('code')?.textContent).toContain('ON_WATCHLIST'));
    expect(screen.queryByRole('button', { name: 'Retry query definition' })).toBeNull();
    expect(sources).toHaveLength(1);
    expect(sources[0].closed).toBe(false);
    expect(backend.requests.slice(before)).toEqual([
      { method: 'GET', path: queryPath, body: undefined },
      { method: 'GET', path: queryPath, body: undefined },
    ]);
  });

  it('aborts a closed inspector and ignores late content before opening a fresh read', async () => {
    const ready = gate();
    let inspecting = false;
    let signal: AbortSignal | null | undefined;
    let reads = 0;
    await renderTrading(new SyntheticTrading(), async (url, init) => {
      if (inspecting && url.pathname === queryPath) {
        signal = init?.signal;
        reads++;
        if (reads === 1) await ready.promise;
      }
    });
    inspecting = true;
    fireEvent.click(within(panel('Watchlist')).getByRole('button', { name: 'View code' }));
    await screen.findByText('Loading query definition...');
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(signal?.aborted).toBe(true);
    await act(async () => ready.resolve());
    expect(screen.queryByRole('dialog')).toBeNull();
    fireEvent.click(within(panel('Watchlist')).getByRole('button', { name: 'View code' }));
    await waitFor(() => expect(screen.getByRole('dialog').querySelector('code')?.textContent).toContain('ON_WATCHLIST'));
    expect(reads).toBe(2);
  });

  it('preserves fullscreen markup, shared sorting, live animations and row actions without another subscription', async () => {
    const { backend, sources } = await renderTrading();
    const original = panel('Watchlist');
    const before = backend.requests.length;
    const frames: FrameRequestCallback[] = [];
    vi.mocked(window.requestAnimationFrame).mockImplementation(callback => frames.push(callback));
    fireEvent.click(within(original).getByRole('button', { name: 'Expand table' }));
    act(() => { frames.shift()?.(0); });
    act(() => { frames.shift()?.(16); });
    const expanded = document.querySelector<HTMLElement>('.drasi-query-table--expanded');
    if (!expanded) throw new Error('Expected expanded card');
    expect(original.className).toContain('drasi-query-table--hidden');
    expect(document.body.getAttribute('data-scroll-locked')).toBe('1');
    expect(expanded.closest<HTMLElement>('[role="dialog"]')?.style.width).toBe('calc(100vw - 64px)');
    expect(within(expanded).getByRole('heading').className).toContain('drasi-query-table__title--expanded');
    const header = within(expanded).getByRole('columnheader', { name: 'Symbol' });
    expect(header.className).toContain('drasi-query-table__heading--expanded');
    fireEvent.click(within(header).getByRole('button'));
    expect(within(expanded).getAllByRole('row')[1].textContent).toContain('MSFT');
    expect(original.querySelector('tbody tr')?.textContent).toContain('MSFT');
    act(() => backend.changePrice('AAPL', 111));
    const apple = within(expanded).getByRole('row', { name: /^AAPL / });
    expect(apple.textContent).toContain('$111.00');
    expect(apple.className).toContain('drasi-row--up');
    expect(original.querySelector('tbody .drasi-row--up')?.textContent).toContain('AAPL');
    expect(backend.requests.slice(before)).toEqual([]);
    expect(sources).toHaveLength(1);
    fireEvent.click(within(apple).getByRole('button', { name: 'Remove from watchlist' }));
    await waitFor(() => expect(within(expanded).queryByRole('row', { name: /^AAPL / })).toBeNull());
    fireEvent.click(within(expanded).getByRole('button', { name: 'Collapse table' }));
    await waitFor(() => expect(document.querySelector('.drasi-query-table--expanded')).toBeNull());
    expect(document.body.style.overflow).toBe('');
    expect(within(original).getByRole('columnheader', { name: 'Symbol' }).getAttribute('aria-sort')).toBe('descending');
  });

  it('retains fullscreen code controls and restores scrolling when its shared query faults', async () => {
    const { sources } = await renderTrading();
    const normal = panel('Watchlist');
    fireEvent.click(within(normal).getByRole('button', { name: 'Expand table' }));
    const expanded = document.querySelector<HTMLElement>('.drasi-query-table--expanded');
    if (!expanded) throw new Error('Expected expanded card');
    fireEvent.click(within(expanded).getByRole('button', { name: 'View code' }));
    await waitFor(() => expect(screen.getByRole('dialog').querySelector('code')?.textContent).toContain('ON_WATCHLIST'));
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(document.body.getAttribute('data-scroll-locked')).toBe('1');
    act(() => sources[0].onmessage?.(new MessageEvent('message', { data: 'malformed JSON' })));
    await waitFor(() => expect(document.querySelector('.drasi-query-table--expanded')).toBeNull());
    expect(document.body.style.overflow).toBe('');
    expect(within(normal).getByRole('alert').textContent).toContain('Showing last known data.');
    expect(within(normal).getByRole('button', { name: 'Retry connection' })).not.toBeNull();
  });

  it('cleans the app-owned expansion frames and collapse timer on unmount', async () => {
    const app = await renderTrading();
    fireEvent.click(within(panel('Watchlist')).getByRole('button', { name: 'Expand table' }));
    fireEvent.click(screen.getByRole('button', { name: 'Collapse table' }));
    app.unmount();
    expect(window.cancelAnimationFrame).toHaveBeenCalledWith(1);
    expect(document.body.style.overflow).toBe('');
  });

  it('preserves Escape and backdrop collapse with the existing transition duration', async () => {
    await renderTrading();
    const normal = panel('Watchlist');
    fireEvent.click(within(normal).getByRole('button', { name: 'Expand table' }));
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(document.querySelector('.drasi-query-table--expanded')).not.toBeNull();
    await waitFor(() => expect(document.querySelector('.drasi-query-table--expanded')).toBeNull());
    fireEvent.click(within(normal).getByRole('button', { name: 'Expand table' }));
    const backdrop = document.querySelector('.drasi-query-table__backdrop');
    if (!backdrop) throw new Error('Expected backdrop');
    await userEvent.setup().click(backdrop);
    await waitFor(() => expect(document.querySelector('.drasi-query-table--expanded')).toBeNull());
    expect(document.body.style.overflow).toBe('');
  });

  it('requires neither an inspector nor header controls when the Trading wrapper is not opted in', () => {
    render(<DrasiClientProvider value={{ client: null, initialized: false, error: null, retry: () => {} }}>
      <TradingQueryTable
        queryId="watchlist-query" queryOptions={tradingQueryOptions('watchlist-query')}
        rowKey={row => row.symbol} columns={[{ key: 'symbol', label: 'Symbol' }]}
      />
    </DrasiClientProvider>);
    expect(screen.queryByRole('button', { name: 'View code' })).toBeNull();
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});

describe('Trading tutorial presentation', () => {
  it('updates displayed/copied content during an open viewer instead of freezing Loading', async () => {
    const user = userEvent.setup();
    const props = { isOpen: true, onClose: vi.fn(), title: 'Example', reactCode: 'consumer v1', cypherQuery: 'Loading query definition...' };
    const dialog = render(<CodeViewerDialog {...props} />);
    dialog.rerender(<CodeViewerDialog {...props} reactCode="consumer v2" cypherQuery="MATCH (n) RETURN n" />);
    expect(screen.getByText('MATCH (n) RETURN n')).not.toBeNull();
    await user.click(screen.getByRole('button', { name: 'Copy' }));
    expect(await navigator.clipboard.readText()).toBe('MATCH (n) RETURN n');
    await user.click(screen.getByRole('tab', { name: 'React Code' }));
    await user.click(screen.getByRole('button', { name: 'Copied!' }));
    expect(await navigator.clipboard.readText()).toBe('consumer v2');
    fireEvent.click(screen.getByText('consumer v2'));
    expect(props.onClose).not.toHaveBeenCalled();
    await user.click(document.querySelector('.drasi-code-dialog__overlay')!);
    expect(props.onClose).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(props.onClose).toHaveBeenCalledTimes(2);
  });

  it('handles copy failure visibly in the existing console channel and resets feedback after success', async () => {
    vi.useFakeTimers();
    userEvent.setup();
    const writeText = vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValue(new Error('Clipboard denied'));
    const log = vi.spyOn(console, 'error').mockImplementation(() => {});
    const props = { isOpen: true, onClose: () => {}, title: 'Example', reactCode: '', cypherQuery: 'MATCH (n)' };
    const dialog = render(<CodeViewerDialog {...props} />);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Copy' })));
    expect(log).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('Copied!')).toBeNull();
    writeText.mockResolvedValue(undefined);
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Copy' })));
    act(() => vi.advanceTimersByTime(2000));
    expect(screen.queryByText('Copied!')).toBeNull();
    dialog.rerender(<CodeViewerDialog {...props} isOpen={false} />);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('keeps query text, source/join order and optional configuration fields in copied definitions', () => {
    const config: QueryConfig = {
      id: 'inspect', queryLanguage: 'GQL', autoStart: false,
      query: '  MATCH (a)\nRETURN a  ',
      sources: [{ sourceId: 'events', pipeline: ['parse'], nodes: ['A'], relations: ['R'] }, { sourceId: 'other', pipeline: [], nodes: [], relations: [] }],
      joins: [{ id: 'JOIN', keys: [{ label: 'A', property: 'id' }, { label: 'B', property: 'id' }] }],
      middleware: [], enableBootstrap: true, bootstrapBufferSize: 100,
      outboxCapacity: 10, bootstrapTimeoutSecs: 30, priorityQueueCapacity: 25,
      dispatchBufferCapacity: 15, dispatchMode: 'Channel', storageBackend: { kind: 'memory' },
    };
    expect(formatQueryConfig(config)).toBe([
      'id: inspect', 'queryLanguage: GQL', 'autoStart: false', '',
      'query: |', '  MATCH (a)', '  RETURN a', '', 'sources:',
      '  - sourceId: events', '    pipeline: [parse]', '    nodes: [A]', '    relations: [R]',
      '  - sourceId: other', '', 'joins:', '  - id: JOIN', '    keys:',
      '      - label: A, property: id', '      - label: B, property: id',
      'enableBootstrap: true', 'bootstrapBufferSize: 100', 'priorityQueueCapacity: 25',
      'dispatchBufferCapacity: 15', 'dispatchMode: Channel', '', 'storageBackend: {\n  "kind": "memory"\n}',
    ].join('\n'));
  });

  it('keeps a lazily mounted inspector pending until an app-owned binding is initialized', () => {
    render(<DrasiClientProvider value={{ client: null, initialized: false, error: null, retry: () => {} }}>
      <QueryInspector queryId="pending" title="Pending" codeSnippet="usage" onClose={() => {}} />
    </DrasiClientProvider>);
    expect(screen.getByText('Loading query definition...')).not.toBeNull();
    expect(screen.queryByRole('link')).toBeNull();
  });

  it('routes a shared inspector failure to the connection owner instead of repeating a local read', () => {
    const retry = vi.fn();
    const error = new DrasiError('SERVER_UNAVAILABLE');
    render(<DrasiClientProvider value={{ client: null, initialized: false, error, retry }}>
      <QueryInspector queryId="pending" title="Pending" codeSnippet="usage" onClose={() => {}} />
    </DrasiClientProvider>);
    fireEvent.click(screen.getByRole('button', { name: 'Retry connection' }));
    expect(retry).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('button', { name: 'Retry query definition' })).toBeNull();
  });
});
