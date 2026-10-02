// @vitest-environment jsdom
import { StrictMode, act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { Lab } from './App';
import { mockViews } from './mock/MockApp';
import { snapshot } from './mock/scenarios';
import { architectureEdges, architectureNodes } from './architectureData';

let root: Root;
const send = vi.fn(), retry = vi.fn();
function element<T extends Element>(selector: string): T {
  const found = document.querySelector<T>(selector);
  if (!found) throw new Error(`Missing ${selector}`);
  return found;
}
async function click(selector: string) {
  await act(async () => element(selector).dispatchEvent(new MouseEvent('click', { bubbles: true })));
}
async function key(selector: string, value: string) {
  await act(async () => element(selector).dispatchEvent(new KeyboardEvent('keydown', { key: value, bubbles: true, cancelable: true })));
}
beforeEach(async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('Overlay must not request data'));
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value() {
    this.open = true;
    this.querySelector('[autofocus]')?.focus();
  } });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', { configurable: true, value() { this.open = false; } });
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
  send.mockReset(); retry.mockReset();
  await act(async () => root.render(<StrictMode><Lab views={mockViews(snapshot('healthy'), 'healthy', retry)}
    connection={{ initialized: true, error: null, retry }}
    command={{ send, pending: false, notice: null, error: null }}/></StrictMode>));
});
afterEach(async () => {
  await act(async () => root.unmount());
  expect(send).not.toHaveBeenCalled();
  expect(retry).not.toHaveBeenCalled();
  expect(fetch).not.toHaveBeenCalled();
  vi.restoreAllMocks(); vi.unstubAllGlobals(); document.body.replaceChildren();
});

it('uses a title-adjacent info button and the shared page/header/title classes in the overlay', async () => {
  const title = element('.lab-app > .lab-header h1');
  const trigger = element<HTMLButtonElement>('.architecture-toggle');
  expect(title.nextElementSibling).toBe(trigger);
  expect(trigger.textContent).toBe('i');
  expect(trigger.getAttribute('aria-label')).toBe('Information about this demo');
  expect(trigger.title).toBe('Architecture Overview');
  expect(trigger.getAttribute('aria-expanded')).toBe('false');
  expect(document.querySelector('.header-tools .architecture-toggle')).toBeNull();
  await click('.architecture-toggle');
  expect(trigger.getAttribute('aria-expanded')).toBe('true');
  expect(element('#gpu-architecture-overlay').id).toBe(trigger.getAttribute('aria-controls'));
  expect(element('.gpu-architecture-page').classList.contains('lab-page')).toBe(true);
  expect(element('.gpu-architecture-page > .lab-header .lab-title h1').textContent).toBe(title.textContent);
  expect(document.querySelector('.gpu-architecture-intro h1')).toBeNull();
  expect(element('.gpu-architecture-close').getAttribute('aria-label')).toBe('Close scenario and architecture');
  expect(element('.gpu-architecture-close svg').getAttribute('aria-hidden')).toBe('true');
  await click('.gpu-architecture-close');
  expect(trigger.getAttribute('aria-expanded')).toBe('false');
});

it('opens the scenario first and keeps dashboard selection/expansion across close and reopen', async () => {
  await click('.analysis-toggle');
  const picker = element<HTMLSelectElement>('#preset');
  await act(async () => {
    picker.value = 'fragmentation';
    picker.dispatchEvent(new Event('change', { bubbles: true }));
  });
  const trigger = element<HTMLButtonElement>('.architecture-toggle');
  trigger.focus();
  document.body.style.overflow = 'auto';
  await click('.architecture-toggle');
  expect(element<HTMLDialogElement>('.gpu-architecture-overlay').open).toBe(true);
  expect(document.body.style.overflow).toBe('hidden');
  expect(element('.gpu-architecture-intro').textContent).toContain('allocation of AI workloads');
  expect(document.querySelectorAll('[data-node]')).toHaveLength(architectureNodes.length);
  expect(document.querySelectorAll('[data-edge]')).toHaveLength(architectureEdges.length);
  await click('.gpu-architecture-close');
  expect(document.querySelector('dialog')).toBeNull();
  expect(document.activeElement).toBe(trigger);
  expect(document.body.style.overflow).toBe('auto');
  expect(picker.value).toBe('fragmentation');
  expect(element('.analysis-toggle').getAttribute('aria-expanded')).toBe('true');
  await click('.architecture-toggle');
  expect(document.querySelector('#gpu-architecture-details')).toBeNull();
  await key('.gpu-architecture-overlay', 'Escape');
  expect(document.activeElement).toBe(trigger);
});

it('introduces the scenario, Drasi composition and the implementation and role of each transformer', async () => {
  await click('.architecture-toggle');
  const intro = element('.gpu-architecture-intro');
  expect(intro.textContent).toContain('simulated cluster of GPUs');
  expect(intro.textContent).toContain('Built with Drasi');
  expect(intro.textContent).toContain('sources, continuous queries, transformers and reactions');
  const cards = [...intro.querySelectorAll('.gpu-transformer-cards > div')];
  expect(cards.map(card => card.querySelector('h2')?.textContent)).toEqual([
    'Policy evaluator', 'Placement solver', 'Resilience assessor', 'Telemetry simulator',
  ]);
  for (const [index, terms] of [
    ['Regorus', 'Rego', 'regional clusters'],
    ['good_lp', 'microlp', 'workload replicas'],
    ['placement solver', 'loss of each VM or region', 'remaining GPUs'],
    ['timer-driven Rust', 'generates telemetry', 'saved allocations'],
  ].entries()) {
    for (const term of terms) expect(cards[index].textContent).toContain(term);
    expect(cards[index].querySelector('p')?.textContent?.match(/\.(?:\s|$)/g)).toHaveLength(2);
  }
  expect(element('.gpu-architecture-board').textContent).toContain('RESULTS AND DASHBOARD');
  expect(element('.gpu-architecture-board').textContent).not.toMatch(/no hypothetical writes|observe and explain/i);
});

it('selects every node and edge on click with complete detail and no hover-only state', async () => {
  await click('.architecture-toggle');
  for (const [attribute, items] of [['node', architectureNodes], ['edge', architectureEdges]] as const) {
    for (const item of items) {
      const selector = `[data-${attribute}="${item.id}"]`;
      await act(async () => element(selector).dispatchEvent(new MouseEvent('mouseover', { bubbles: true })));
      expect(document.querySelector('#gpu-architecture-details')).toBeNull();
      await click(selector);
      expect(element('#gpu-detail-title').textContent).toBe(item.title);
      expect(element('#gpu-architecture-details').textContent).toContain(item.detail);
      expect(element('#gpu-architecture-details').textContent).not.toMatch(/undefined|\[object Object\]/);
      expect(document.activeElement).toBe(element('#gpu-detail-title'));
      await click('.gpu-detail-close');
      expect(document.querySelector('#gpu-architecture-details')).toBeNull();
    }
  }
});

it('supports edge keyboard selection, two-level Escape, focus return and switching selection', async () => {
  const trigger = element<HTMLButtonElement>('.architecture-toggle');
  trigger.focus();
  await click('.architecture-toggle');
  const edge = '[data-edge="reports"]';
  await key(edge, 'Enter');
  expect(element('#gpu-detail-title').textContent).toBe('Simulator → observed capacity query');
  await key('#gpu-detail-title', 'Escape');
  expect(document.querySelector('#gpu-architecture-details')).toBeNull();
  expect(document.activeElement).toBe(element(edge));
  await key(edge, ' ');
  expect(element('#gpu-detail-title').textContent).toBe('Simulator → observed capacity query');
  await click('[data-node="policy"]');
  expect(element('#gpu-detail-title').textContent).toBe('Policy evaluator');
  await act(async () => element('dialog').dispatchEvent(new Event('cancel', { cancelable: true })));
  expect(document.querySelector('#gpu-architecture-details')).toBeNull();
  expect(document.activeElement).toBe(element('[data-node="policy"]'));
  await key('dialog', 'Escape');
  expect(document.querySelector('dialog')).toBeNull();
  expect(document.activeElement).toBe(trigger);
});

it('wraps Tab within the modal without targeting hidden connection-index buttons', async () => {
  await click('.architecture-toggle');
  const rect = new DOMRect(0, 0, 20, 20);
  vi.spyOn(HTMLElement.prototype, 'getClientRects').mockReturnValue({
    0: rect, length: 1, item: () => rect, [Symbol.iterator]: () => [rect][Symbol.iterator](),
  });
  const close = element<HTMLButtonElement>('.gpu-architecture-close');
  close.focus();
  await act(async () => close.dispatchEvent(new KeyboardEvent('keydown', {
    key: 'Tab', shiftKey: true, bubbles: true, cancelable: true,
  })));
  expect(document.activeElement).toBe(element('.gpu-architecture-index > summary'));
  await key('.gpu-architecture-index > summary', 'Tab');
  expect(document.activeElement).toBe(close);
});
