// @vitest-environment jsdom
import { act, useRef } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useReplicaMotion } from './replicaMotion';
import { buildReplicaView, replicaTiles, type ReplicaView } from './replicas';
import { mockViews } from './mock/MockApp';
import { snapshot } from './mock/scenarios';

let root: Root;
let animate: ReturnType<typeof vi.fn>;
let cancel: ReturnType<typeof vi.fn>;
let reduced = false;
const listeners = new Set<() => void>();
const originalAnimate = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'animate');
const view = (scene: string) => buildReplicaView(mockViews(snapshot(scene), scene, () => {}));
function Harness({ state, enabled = true }: { state: ReplicaView; enabled?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const cues = useReplicaMotion(ref, state, enabled);
  return <div ref={ref}>
    {replicaTiles(state).map(tile => <div key={`${tile.gpuId}/${tile.replica.id}`}
      data-allocation-gpu={tile.gpuId} data-hierarchy-id={tile.replica.id} data-allocation-section={tile.section}/>)}
    <output>{cues.map(cue => `${cue.kind}:${cue.id}`).join(',')}</output>
  </div>;
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  reduced = false;
  listeners.clear();
  vi.stubGlobal('matchMedia', () => ({
    matches: reduced,
    addEventListener: (_name: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_name: string, listener: () => void) => listeners.delete(listener),
  }));
  cancel = vi.fn();
  animate = vi.fn(() => ({ cancel, onfinish: null, oncancel: null }));
  Object.defineProperty(HTMLElement.prototype, 'animate', { configurable: true, value: animate });
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    return new DOMRect(0, this.dataset.allocationSection === 'planned' ? 10 : 90, 160, 30);
  });
  const host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  expect(vi.getTimerCount()).toBe(0);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
  if (originalAnimate) Object.defineProperty(HTMLElement.prototype, 'animate', originalAnimate);
  else Reflect.deleteProperty(HTMLElement.prototype, 'animate');
  document.body.replaceChildren();
});
describe('replica visual cues', () => {
  it('slides only within one GPU, fades across GPUs, and expires cues without changing placement', async () => {
    await act(async () => root.render(<Harness state={view('plan-committed')}/>));
    expect(animate).not.toHaveBeenCalled();
    await act(async () => root.render(<Harness state={view('plan-applied')}/>));
    expect(animate).toHaveBeenCalledTimes(2);
    const frames = animate.mock.calls.map(call => call[0]);
    expect(frames.filter(value => 'transform' in value[0])).toHaveLength(1);
    expect(frames.filter(value => 'opacity' in value[0])).toHaveLength(1);
    expect(document.querySelector('output')?.textContent).toContain('move:mock-chat-alpha/0');
    const assignments = [...document.querySelectorAll('[data-allocation-gpu]')].map(node => node.outerHTML);
    await act(async () => vi.advanceTimersByTime(4001));
    expect(document.querySelector('output')?.textContent).toBe('');
    expect([...document.querySelectorAll('[data-allocation-gpu]')].map(node => node.outerHTML)).toEqual(assignments);
  });
  it('does not replay animations after reconnect, a new epoch, or merely fresh reports', async () => {
    await act(async () => root.render(<Harness state={view('plan-committed')}/>));
    await act(async () => root.render(<Harness state={{ ...view('plan-committed'), live: false }}/>));
    await act(async () => root.render(<Harness state={view('plan-applied')}/>));
    expect(animate).not.toHaveBeenCalled();
    await act(async () => root.render(<Harness state={view('plan-confirmed')}/>));
    expect(animate).not.toHaveBeenCalled();
    await act(async () => root.render(<Harness state={{ ...view('plan-committed'), epoch: 'replacement' }}/>));
    expect(animate).not.toHaveBeenCalled();
  });
  it('cancels motion immediately for reduced motion and keeps a static change cue', async () => {
    await act(async () => root.render(<Harness state={view('plan-committed')}/>));
    await act(async () => root.render(<Harness state={view('plan-applied')}/>));
    expect(animate).toHaveBeenCalledTimes(2);
    await act(async () => { reduced = true; listeners.forEach(listener => listener()); });
    expect(cancel).toHaveBeenCalledTimes(2);
    expect(document.querySelector('output')?.textContent).not.toBe('');
    animate.mockClear();
    await act(async () => root.render(<Harness state={view('policy-fenced')}/>));
    expect(animate).not.toHaveBeenCalled();
  });
  it('clears decorations and timers when disabled or disconnected', async () => {
    await act(async () => root.render(<Harness state={view('plan-committed')}/>));
    await act(async () => root.render(<Harness state={view('plan-applied')}/>));
    await act(async () => root.render(<Harness state={view('plan-applied')} enabled={false}/>));
    expect(document.querySelector('output')?.textContent).toBe('');
    expect(vi.getTimerCount()).toBe(0);
    expect(cancel).toHaveBeenCalledTimes(2);
    await act(async () => root.render(<Harness state={view('plan-applied')}/>));
    expect(animate).toHaveBeenCalledTimes(2);
    await act(async () => root.render(<Harness state={{ ...view('plan-applied'), live: false }}/>));
    expect(document.querySelector('output')?.textContent).toBe('');
  });
});
