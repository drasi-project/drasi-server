// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { LiveApp } from './App';
import { fixture, type MockRows } from './mock/fixtures';
import type { QueryId } from './rows';

const sdk = vi.hoisted(() => ({
  initialized: true, connected: true, transportError: null as Error | null, retry: vi.fn(),
}));
let rows: MockRows;
let root: Root;
vi.mock('@drasi/react/react', async importOriginal => ({
  ...await importOriginal<typeof import('@drasi/react/react')>(),
  useDrasiClient: () => ({ ...sdk, error: null }),
  useDrasiConnectionStatus: () => ({ connected: sdk.connected, error: sdk.transportError ?? undefined }),
  useDrasiQuery: (id: QueryId) => ({
    data: sdk.initialized ? rows[id] : null, loading: !sdk.initialized,
    stale: false, error: null, status: sdk.initialized ? 'live' : 'initial-loading', retry: vi.fn(),
  }),
}));
function addButton(): HTMLButtonElement {
  const button = [...document.querySelectorAll('button')].find(b => b.textContent === 'Add workload');
  if (!button) throw new Error('Missing Add workload button');
  return button;
}
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.spyOn(navigator, 'onLine', 'get').mockReturnValue(true);
  rows = fixture('baseline');
  sdk.initialized = true;
  sdk.connected = true;
  sdk.transportError = null;
  sdk.retry.mockReset().mockImplementation(() => { sdk.initialized = false; });
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  vi.restoreAllMocks(); vi.unstubAllGlobals(); document.body.replaceChildren();
});
it('gates edits immediately when the browser reports offline even with retained live feeds', async () => {
  await act(async () => root.render(<LiveApp/>));
  expect(addButton().disabled).toBe(false);
  await act(async () => window.dispatchEvent(new Event('offline')));
  expect(addButton().disabled).toBe(true);
  expect(document.body.textContent).toContain('Updates are unavailable or out of date.');
  expect(sdk.retry).not.toHaveBeenCalled();
  await act(async () => window.dispatchEvent(new Event('online')));
  expect(sdk.retry).toHaveBeenCalledOnce();
  expect(addButton().disabled).toBe(true);
  sdk.initialized = true;
  await act(async () => root.render(<LiveApp/>));
  expect(addButton().disabled).toBe(false);
});
it('starts gated when the browser is already offline', async () => {
  vi.spyOn(navigator, 'onLine', 'get').mockReturnValue(false);
  await act(async () => root.render(<LiveApp/>));
  expect(addButton().disabled).toBe(true);
});
it('shows the SDK retry error before initial connection eventually succeeds or fails', async () => {
  sdk.initialized = false;
  sdk.connected = false;
  sdk.transportError = new Error('Query metadata request failed: 503');
  await act(async () => root.render(<LiveApp/>));
  expect(document.body.textContent).toContain('Query metadata request failed: 503');
  expect(document.body.textContent).toContain('Updates are unavailable or out of date.');
  expect(addButton().disabled).toBe(true);
  expect([...document.querySelectorAll('button')].find(b => b.textContent === 'Reset scenario')?.disabled).toBe(false);
});
it('gates retained live rows during transport reconnect and reopens only after connection returns', async () => {
  await act(async () => root.render(<LiveApp/>));
  expect(addButton().disabled).toBe(false);
  sdk.connected = false;
  sdk.transportError = new Error('Connection is being retried');
  await act(async () => root.render(<LiveApp/>));
  expect(addButton().disabled).toBe(true);
  expect(document.body.textContent).toContain('Connection is being retried');
  sdk.transportError = null;
  await act(async () => root.render(<LiveApp/>));
  expect(addButton().disabled).toBe(true);
  sdk.connected = true;
  await act(async () => root.render(<LiveApp/>));
  expect(addButton().disabled).toBe(false);
});
it('observes connectivity changes between rendering and listener registration', async () => {
  vi.spyOn(navigator, 'onLine', 'get').mockReturnValueOnce(true).mockReturnValue(false);
  await act(async () => root.render(<LiveApp/>));
  expect(addButton().disabled).toBe(true);
});
it('removes browser connectivity listeners on unmount', async () => {
  await act(async () => root.render(<LiveApp/>));
  await act(async () => root.unmount());
  window.dispatchEvent(new Event('online'));
  expect(sdk.retry).not.toHaveBeenCalled();
});
