// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import MockApp, { mockViews } from './MockApp';
import { Lab } from '../App';
import { scenarios, snapshot } from './scenarios';

let root: Root;
function element<T extends Element>(selector: string): T {
  const node = document.querySelector<T>(selector);
  if (!node) throw new Error(`Missing element: ${selector}`);
  return node;
}
function button(label: string, within: ParentNode = document): HTMLButtonElement {
  const node = [...within.querySelectorAll('button')].find(b => b.textContent === label || b.getAttribute('aria-label') === label);
  if (!node) throw new Error(`Missing button: ${label}`);
  return node;
}
async function click(label: string) { await act(async () => button(label).click()); }
async function toggleWorkloads() { await act(async () => element<HTMLElement>('.workload-body > summary').click()); }
function loadStyles() {
  const stylesheet = document.createElement('style');
  stylesheet.textContent = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), '../style.css'), 'utf8');
  document.body.append(stylesheet);
}
function resolveStyle(value: string) {
  const tokens = getComputedStyle(document.documentElement);
  return value.replace(/var\((--[\w-]+)\)/g, (_, token: string) => tokens.getPropertyValue(token).trim());
}
function visualStyle(node: Element, property: string) {
  const declaration = document.createElement('span').style;
  declaration.setProperty(property, resolveStyle(getComputedStyle(node).getPropertyValue(property)));
  return declaration.getPropertyValue(property);
}
function contrast(first: string, second: string) {
  const luminance = (hex: string) => {
    expect(hex).toMatch(/^#[0-9a-f]{6}$/i);
    const values = [1, 3, 5].map(index => parseInt(hex.slice(index, index + 2), 16) / 255)
      .map(channel => channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4);
    return values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722;
  };
  const left = luminance(first), right = luminance(second);
  return (Math.max(left, right) + 0.05) / (Math.min(left, right) + 0.05);
}
async function select(selector: string, value: string) {
  await act(async () => {
    const node = element<HTMLSelectElement>(selector);
    node.value = value;
    node.dispatchEvent(new Event('change', { bubbles: true }));
  });
}
async function submit() {
  await act(async () => { element<HTMLFormElement>('dialog form').dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); });
}
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('Mock must not request the backend'));
  vi.spyOn(window, 'confirm').mockReturnValue(true);
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value() { this.open = true; } });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', { configurable: true, value() { this.open = false; } });
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: vi.fn() });
  const host = document.createElement('div'); document.body.append(host); root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  expect(fetch).not.toHaveBeenCalled();
  vi.restoreAllMocks(); vi.unstubAllGlobals(); document.body.replaceChildren();
});

describe('mock controls and evidence interactions', () => {
  it('shares panel typography, gutters, borders and corners across the main view and analysis', async () => {
    loadStyles();
    await act(async () => root.render(<MockApp initialSnapshot="regional"/>));
    await click('Global analysis');
    const panels = [...document.querySelectorAll('.cluster, .workload-panel, .activity, .analysis-drawer, .analysis-content > .panel')];
    for (const property of ['border-top-color', 'border-radius', 'background-color']) {
      const values = panels.map(panel => visualStyle(panel, property));
      expect(values[0]).not.toBe('');
      expect(new Set(values).size).toBe(1);
    }
    for (const panel of document.querySelectorAll('.cluster, .workload-panel, .activity')) expect(visualStyle(panel, 'margin-bottom')).toBe('12px');
    expect(visualStyle(element('.analysis-content > .panel:last-child'), 'margin-bottom')).toBe('0px');
    for (const property of ['padding', 'background-color', 'border-bottom-color']) {
      expect(visualStyle(element('.workload-heading'), property)).toBe(visualStyle(element('.cluster-title'), property));
      expect(visualStyle(element('.analysis-heading'), property)).toBe(visualStyle(element('.cluster-title'), property));
    }
    for (const heading of document.querySelectorAll('.workload-heading h3, .cluster-title h3, .analysis-heading h2, .analysis-content h3')) {
      expect(visualStyle(heading, 'font-size')).toBe('15px');
      expect(visualStyle(heading, 'font-weight')).toBe('600');
    }
  });
  it('shares text-control styling while keeping compact header and GPU icon controls distinct', async () => {
    loadStyles();
    await act(async () => root.render(<MockApp/>));
    const actions = [button('Add workload'), button('Edit policy'), button('Add ready VM'), button('Power off VM')];
    for (const property of ['font-size', 'line-height', 'padding', 'border-radius', 'min-height', 'background-color', 'border-top-color']) {
      const values = actions.map(action => visualStyle(action, property));
      expect(values[0]).not.toBe('');
      expect(new Set(values).size).toBe(1);
    }
    const header = [button('Global analysis'), button('System status'), button('Reload example state'), element('.reading-guide > summary')];
    for (const property of ['font-size', 'line-height', 'padding', 'border-radius', 'min-height']) {
      const values = header.map(action => visualStyle(action, property));
      expect(values[0]).not.toBe('');
      expect(new Set(values).size).toBe(1);
    }
    for (const property of ['font-size', 'line-height', 'padding', 'border-radius', 'border-top-color']) {
      expect(visualStyle(element('.workload-chip'), property)).toBe(visualStyle(element('.vm-preview'), property));
    }
    expect(visualStyle(button('Pause reports'), 'width')).toBe('32px');
    expect(visualStyle(element('.policy-indicator'), 'width')).toBe('20px');
    expect(visualStyle(element('.policy-indicator'), 'min-height')).toBe('20px');
  });
  it('keeps badge geometry identical for healthy, overdue and unknown report states', async () => {
    loadStyles();
    await act(async () => root.render(<MockApp initialSnapshot="reports-expired"/>));
    const healthy = element('.gpu .badge.good'), overdue = element('.gpu .badge.warning');
    for (const property of ['font-size', 'line-height', 'padding', 'border-radius', 'border-top-width']) {
      expect(visualStyle(healthy, property)).not.toBe('');
      expect(visualStyle(overdue, property)).toBe(visualStyle(healthy, property));
    }
    expect(visualStyle(overdue, 'color')).toBe(visualStyle(element('.mini-gpu.report-unreachable'), 'color'));
    const radius = visualStyle(overdue, 'border-radius');
    await select('#snapshot', 'bootstrap');
    expect(visualStyle(element('.gpu .badge.warning'), 'border-radius')).toBe(radius);
    expect(visualStyle(element('.gpu .badge.warning'), 'color')).toBe(visualStyle(element('.mini-gpu.report-unknown'), 'color'));
  });
  it('uses the same denial colour in policy inspection and GPU indicators without changing report status', async () => {
    loadStyles();
    await act(async () => root.render(<MockApp initialSnapshot="regional"/>));
    await act(async () => button('assistant', element('.workload-summary')).click());
    const denied = element<HTMLElement>('.destination-deny');
    await act(async () => denied.closest('details')!.querySelector('summary')!.click());
    await act(async () => button('Policy details: assistant · policy: Not allowed', denied).click());
    expect(element('.eligibility .badge.danger').textContent).toBe('Not allowed');
    expect(visualStyle(element('.eligibility .badge.danger'), 'color')).toBe(visualStyle(element('.policy-indicator.policy-deny'), 'color'));
    expect(denied.querySelector('.badge.good')?.textContent).toBe('Recent report');
  });
  it('keeps dialog errors distinct from explanatory text and aligns dialog titles and close controls', async () => {
    loadStyles();
    const send = vi.fn(async () => { throw new Error('Example command failure'); });
    await act(async () => root.render(<Lab views={mockViews(snapshot('healthy'), 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    await click('Global analysis');
    await click('Add workload');
    expect(visualStyle(element('#dialog-title'), 'font-size')).toBe(visualStyle(element('.analysis-heading h2'), 'font-size'));
    for (const property of ['width', 'height', 'padding', 'font-size']) {
      expect(visualStyle(button('Close dialog'), property)).toBe(visualStyle(button('Close global analysis'), property));
    }
    element<HTMLInputElement>('input[name="name"]').value = 'example';
    await submit();
    expect(visualStyle(element('dialog p.error'), 'color')).not.toBe(visualStyle(element('dialog fieldset p'), 'color'));
    expect(resolveStyle(getComputedStyle(element('dialog p.error')).color)).toBe(resolveStyle('var(--negative)'));
  });
  it('retains readable text contrast in semantic colours and primary-button hover styling', async () => {
    loadStyles();
    const token = (name: string) => resolveStyle(`var(--${name})`);
    for (const [foreground, background] of [
      ['text-primary', 'surface-panel'], ['text-secondary', 'surface-raised'], ['text-muted', 'surface-raised'],
      ['positive', 'positive-surface'], ['caution', 'caution-surface'], ['negative', 'negative-surface'],
      ['primary-text', 'primary-fill'], ['primary-text', 'primary-hover'],
      ['demo-text', 'demo-surface'],
    ]) expect(contrast(token(foreground), token(background))).toBeGreaterThanOrEqual(4.5);
    const hover = [...document.styleSheets].flatMap(sheet => [...sheet.cssRules])
      .find((rule): rule is CSSStyleRule => rule instanceof CSSStyleRule && rule.selectorText === '.lab-app button.primary:hover:not(:disabled)');
    expect(hover).toBeDefined();
    expect(resolveStyle(hover!.style.getPropertyValue('color'))).toBe(token('primary-text'));
    expect(resolveStyle(hover!.style.getPropertyValue('background'))).toBe(token('primary-hover'));
  });
  it.each(['healthy', 'regional', 'bootstrap'])('starts every panel collapsed in %s', async scene => {
    loadStyles();
    await act(async () => root.render(<MockApp initialSnapshot={scene}/>));
    expect(document.querySelectorAll('details').length).toBeGreaterThan(0);
    expect(document.querySelectorAll('details[open], .gpu-expanded')).toHaveLength(0);
    expect(element<HTMLElement>('#global-analysis').hidden).toBe(true);
    expect(getComputedStyle(element('.workload-summary')).display).toBe('flex');
    for (const preview of document.querySelectorAll('.region-preview')) expect(getComputedStyle(preview).display).toBe('flex');
    for (const details of document.querySelectorAll<HTMLElement>('.gpu-details')) expect(details.hidden).toBe(true);
    const summary = element('.workload-body > summary'), regionSummary = element('.cluster-body > summary');
    expect(summary.parentElement?.tagName).toBe('DETAILS');
    expect(summary.querySelector('svg, button')).toBeNull();
    for (const property of ['display', 'font-size', 'color', 'padding']) {
      expect(getComputedStyle(summary).getPropertyValue(property)).toBe(getComputedStyle(regionSummary).getPropertyValue(property));
    }
  });
  it('keeps the ordered page header free of demo controls and warnings', async () => {
    loadStyles();
    await act(async () => root.render(<MockApp/>));
    expect(document.querySelector('.intro, .banner, .mock-banner, .flow, .logo')).toBeNull();
    expect(document.querySelectorAll('h1')).toHaveLength(1);
    expect(element('.mode-label').textContent).toBe('MOCK DATA');
    expect(element('.mode-label').closest('.demo-panel')).not.toBeNull();
    expect(document.querySelector('.metrics, .metric, [aria-label="Fleet summary"]')).toBeNull();
    expect(document.querySelector('.bottom-grid')).toBeNull();
    expect(document.querySelector('footer')).toBeNull();
    expect(button('System status').closest('header')).not.toBeNull();
    expect(document.querySelector('dialog')).toBeNull();
    expect(element<HTMLDetailsElement>('.activity').open).toBe(false);
    expect(element<HTMLDetailsElement>('.decision-details').open).toBe(false);
    expect(element('.decision-details').closest('.panel')?.querySelector('h3')?.textContent).toBe('Placement plan');
    expect(element('#snapshot').closest('.demo-panel')).not.toBeNull();
    expect(button('Reload example state').closest('.demo-panel')).not.toBeNull();
    expect(document.querySelector('header .mode-label, header #snapshot, header .demo-panel')).toBeNull();
    expect(element('.lab-header').textContent).not.toMatch(/MOCK DATA|Prepared example|No real GPUs/);
    expect(getComputedStyle(element('.lab-header')).gridTemplateColumns).toBe('minmax(0,1fr) auto minmax(0,1fr)');
    expect(element('.demo-panel').previousElementSibling).toBe(element('.lab-header'));
    expect(element('.lab-layout').previousElementSibling).toBe(element('.demo-panel'));
    expect(visualStyle(element('.demo-panel'), 'background-color')).not.toBe('rgba(0, 0, 0, 0)');
    expect(visualStyle(element('.workload-panel'), 'background-color')).not.toBe('rgba(0, 0, 0, 0)');
    expect(visualStyle(element('.demo-panel'), 'background-color')).not.toBe(visualStyle(element('.workload-panel'), 'background-color'));
    expect(element('.demo-panel').textContent).toContain('Prepared example data, not live results');
    expect(document.querySelector('.toolbar, nav[aria-label="Fleet commands"]')).toBeNull();
    expect(element('#preset').closest('header')).not.toBeNull();
    expect(getComputedStyle(element('.lab-header > .scenario-controls')).justifySelf).toBe('center');
    expect(getComputedStyle(element('.header-tools')).justifySelf).toBe('end');
    expect(getComputedStyle(element('#preset')).width).toBe('auto');
    expect(getComputedStyle(element('#preset')).flexBasis).toBe('auto');
    expect(getComputedStyle(element('#preset')).padding).toBe('2px 4px');
    expect(getComputedStyle(element('#snapshot')).maxWidth).toBe('300px');
    expect(button('Reset scenario').closest('header')).not.toBeNull();
    expect(button('Edit policy').closest('.workload-heading')).toBe(button('Add workload').closest('.workload-heading'));
    expect(button('Add workload').closest('.workload-panel')).not.toBeNull();
    expect(button('Add workload').classList.contains('primary')).toBe(false);
    for (const action of document.querySelectorAll('button')) expect(action.textContent?.trim()).not.toMatch(/^\+/);
    expect(button('Add ready VM').closest('.cluster-title')).not.toBeNull();
    expect(document.querySelector('#workload-filter, .workload-filter')).toBeNull();
    expect(document.querySelector('.fleet > .section-title')).toBeNull();
    expect(element('.fleet').getAttribute('aria-label')).toBe('Regions');
    expect([...document.querySelectorAll('aside h3')].map(node => node.textContent)).toEqual(['Placement plan', 'Recovery after failures']);
    expect(element<HTMLElement>('#global-analysis').hidden).toBe(true);
    expect(button('Global analysis').closest('.header-tools')).toBe(button('System status').closest('.header-tools'));
    expect(element('.header-tools').firstElementChild).toBe(button('System status'));
    expect(button('System status').nextElementSibling).toBe(button('Global analysis'));
    expect(button('Global analysis').nextElementSibling).toBe(element('.reading-guide'));
    expect(document.body.textContent).not.toContain('Where policy allows processing');
    const help = element<HTMLDetailsElement>('.reading-guide');
    expect(help.closest('header')).not.toBeNull();
    expect(help.open).toBe(false);
    await act(async () => element<HTMLElement>('.reading-guide > summary').click());
    expect(help.open).toBe(true);
    expect(help.textContent).toContain('How to read this demo');
    expect(help.textContent).toContain('example customer agreement, not a general GDPR requirement');
    await act(async () => element<HTMLElement>('.reading-guide > summary').click());
    expect(help.open).toBe(false);
  });
  it('opens a default-hidden right column alongside the region and workload panels without requesting analysis', async () => {
    loadStyles();
    const send = vi.fn(async () => {}), retry = vi.fn();
    await act(async () => root.render(<Lab views={mockViews(snapshot('healthy'), 'healthy', retry)}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry }}/>));
    const toggle = button('Global analysis'), drawer = element<HTMLElement>('#global-analysis'), fleet = element('.fleet');
    const layout = element('.lab-layout'), content = element('.lab-content');
    const before = fleet.textContent;
    expect(toggle.getAttribute('aria-controls')).toBe(drawer.id);
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(drawer.hidden).toBe(true);
    expect(getComputedStyle(drawer).display).toBe('none');
    expect(fleet.parentElement).toBe(content);
    expect(content.firstElementChild).toBe(fleet);
    const workloads = element('.workload-panel');
    expect(element('table').closest('.workload-panel')).toBe(workloads);
    expect(element('.activity').parentElement).toBe(content);
    expect(element('.activity').previousElementSibling).toBe(fleet);
    expect([...layout.children]).toEqual([workloads, content, drawer]);
    expect(getComputedStyle(workloads).gridColumn).toBe('1');
    expect(getComputedStyle(workloads).gridRow).toBe('1');
    expect(getComputedStyle(content).gridColumn).toBe('1');
    expect(getComputedStyle(content).gridRow).toBe('2');
    expect(getComputedStyle(drawer).gridColumn).toBe('2');
    expect(getComputedStyle(drawer).gridRow).toBe('2');
    expect(layout.previousElementSibling).toBe(element('.lab-header'));
    expect(getComputedStyle(layout).marginTop).toBe('10px');
    expect(getComputedStyle(toggle).padding).toBe('3px 6px');
    expect(getComputedStyle(layout).display).toBe('grid');
    expect(getComputedStyle(layout).alignItems).toBe('start');
    expect(getComputedStyle(layout).gridTemplateColumns).toBe('minmax(0,1fr) 0px');
    expect(getComputedStyle(layout).columnGap).toBe('0');
    toggle.focus();
    await click('Global analysis');
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(drawer.hidden).toBe(false);
    expect(getComputedStyle(layout).gridTemplateColumns).toBe('minmax(0,1fr) min(380px,38%)');
    expect(getComputedStyle(layout).columnGap).toBe('16px');
    expect(getComputedStyle(drawer).position).toBe('static');
    expect(getComputedStyle(drawer).display).toBe('flex');
    expect(document.activeElement).toBe(button('Close global analysis'));
    expect(fleet.textContent).toBe(before);
    expect(document.body.style.overflow).toBe('');
    const details = drawer.querySelector<HTMLDetailsElement>('.technical-details')!;
    await act(async () => details.querySelector('summary')!.click());
    expect(details.open).toBe(true);
    await click('Close global analysis');
    expect(drawer.hidden).toBe(true);
    expect(getComputedStyle(layout).gridTemplateColumns).toBe('minmax(0,1fr) 0px');
    expect(getComputedStyle(layout).columnGap).toBe('0');
    expect(document.activeElement).toBe(toggle);
    await click('Global analysis');
    expect(details.open).toBe(true);
    await click('Global analysis');
    expect(drawer.hidden).toBe(true);
    expect(send).not.toHaveBeenCalled();
    expect(retry).not.toHaveBeenCalled();
  });
  it('lets expanded analysis content determine the panel height without nested vertical limits', async () => {
    loadStyles();
    await act(async () => root.render(<MockApp initialSnapshot="regional"/>));
    await click('Global analysis');
    const drawer = element<HTMLElement>('#global-analysis');
    expect(getComputedStyle(drawer).height).toBe('auto');
    expect(getComputedStyle(drawer).maxHeight).toBe('none');
    expect(getComputedStyle(drawer).position).toBe('static');
    expect(getComputedStyle(element('.analysis-content')).overflow).toBe('visible');
    const expanders = [...drawer.querySelectorAll('details')];
    expect(expanders.length).toBeGreaterThan(2);
    for (const details of expanders) {
      await act(async () => details.querySelector('summary')!.click());
      expect(details.open).toBe(true);
    }
    for (const content of drawer.querySelectorAll('.panel-content')) {
      expect(getComputedStyle(content).maxHeight).toBe('none');
      expect(getComputedStyle(content).overflow).toBe('visible');
    }
    for (const pre of drawer.querySelectorAll('pre')) expect(getComputedStyle(pre).maxHeight).toBe('none');
    expect(getComputedStyle(drawer).height).toBe('auto');
    expect(getComputedStyle(drawer).maxHeight).toBe('none');
    await act(async () => expanders[0].querySelector('summary')!.click());
    expect(expanders[0].open).toBe(false);
  });
  it('keeps analysis live while allowing fleet interaction without stealing focus on updates', async () => {
    const send = vi.fn(async () => {}), retry = vi.fn();
    const render = (scene: string) => <Lab views={mockViews(snapshot(scene), scene, retry)}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry }}/>;
    await act(async () => root.render(render('healthy')));
    await click('Global analysis');
    expect(element('.analysis-content .panel .badge').textContent).toBe('Confirmed by GPU reports');
    const workload = button('assistant');
    workload.focus();
    await click('assistant');
    expect(document.querySelectorAll('.selected-replica')).toHaveLength(2);
    expect(document.activeElement).toBe(workload);
    await act(async () => root.render(render('reports-expired')));
    expect(element<HTMLElement>('#global-analysis').hidden).toBe(false);
    expect(element('.analysis-content .panel .badge').textContent).not.toBe('Confirmed by GPU reports');
    expect(document.activeElement).toBe(workload);
    await click('Close global analysis');
    await act(async () => root.render(render('healthy')));
    await click('Global analysis');
    expect(element('.analysis-content .panel .badge').textContent).toBe('Confirmed by GPU reports');
    expect(send).not.toHaveBeenCalled();
    expect(retry).not.toHaveBeenCalled();
  });
  it.each(['policy details', 'command', 'system status'])('closes analysis with Escape without intercepting an open %s dialog', async kind => {
    await act(async () => root.render(<MockApp/>));
    await click('Global analysis');
    if (kind === 'policy details') {
      await click('assistant');
      await click('Policy details: assistant · policy: Allowed');
    } else await click(kind === 'system status' ? 'System status' : 'Edit policy');
    await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); });
    expect(element<HTMLElement>('#global-analysis').hidden).toBe(false);
    await act(async () => { element('dialog').dispatchEvent(new Event('cancel', { bubbles: true, cancelable: true })); });
    expect(document.querySelector('dialog')).toBeNull();
    expect(element<HTMLElement>('#global-analysis').hidden).toBe(false);
    await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); });
    expect(element<HTMLElement>('#global-analysis').hidden).toBe(true);
    expect(document.activeElement).toBe(button('Global analysis'));
  });
  it.each(['feed-stale', 'query-error', 'bootstrap'])('allows inspection of %s without claiming current analysis', async scene => {
    await act(async () => root.render(<MockApp initialSnapshot={scene}/>));
    expect(button('Global analysis').disabled).toBe(false);
    await click('Global analysis');
    expect(element<HTMLElement>('#global-analysis').hidden).toBe(false);
    expect(document.querySelector('#global-analysis .badge.good')).toBeNull();
  });
  it.each(['disconnected', 'initializing'])('invalidates displayed analysis when the connection is %s despite fresh query flags', async state => {
    const feeds = mockViews(snapshot('healthy'), 'healthy', () => {}), send = vi.fn(async () => {}), retry = vi.fn();
    const render = (unavailable: boolean) => <Lab views={feeds}
      command={{ send, pending: false, notice: null, error: null }}
      connection={{ initialized: !(unavailable && state === 'initializing'), error: unavailable && state === 'disconnected' ? new Error('Connection lost') : null, retry }}/>;
    await act(async () => root.render(render(false)));
    await click('Global analysis');
    expect(document.querySelector('#global-analysis .badge.good')).not.toBeNull();
    await act(async () => root.render(render(true)));
    expect(Object.values(feeds).every(view => !view.stale && !view.error && !view.loading)).toBe(true);
    expect(button('Global analysis').disabled).toBe(false);
    expect(element<HTMLElement>('#global-analysis').hidden).toBe(false);
    expect(document.querySelector('#global-analysis .badge.good')).toBeNull();
    expect([...document.querySelectorAll('#global-analysis .section-title > span')].map(node => node.textContent)).toEqual(['Out of date', 'Out of date', 'Out of date']);
    await act(async () => root.render(render(false)));
    expect(document.querySelector('#global-analysis .badge.good')).not.toBeNull();
    expect(send).not.toHaveBeenCalled();
    expect(retry).not.toHaveBeenCalled();
  });
  it('orders Activity by exact sequence within the current run without modifying source rows', async () => {
    const rows = snapshot('healthy'), base = rows['ui-timeline'][0], send = vi.fn(async () => {});
    rows['ui-timeline'] = [
      { ...base, event_id: `${base.observation_epoch}/9007199254740992`, event_sequence: '9007199254740992', message: 'Earlier event' },
      { ...base, event_id: 'old-run/1', observation_epoch: 'old-run', time_ms: Number(base.time_ms) + 10000, message: 'Old run event' },
      { ...base, event_id: `${base.observation_epoch}/9007199254740993`, event_sequence: '9007199254740993', message: 'Newest event' },
    ];
    const before = rows['ui-timeline'].map(row => row.event_id);
    await act(async () => root.render(<Lab views={mockViews(rows, 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    const activity = element<HTMLDetailsElement>('.activity');
    expect(activity.open).toBe(false);
    expect(activity.querySelector('summary')?.textContent).toContain('Newest event');
    await act(async () => activity.querySelector('summary')!.click());
    expect([...activity.querySelectorAll('.timeline-event > p')].map(node => node.textContent)).toEqual(['Newest event', 'Earlier event', 'Old run event']);
    expect(activity.querySelectorAll('.timeline-event')[2].textContent).toContain('previous run');
    expect(rows['ui-timeline'].map(row => row.event_id)).toEqual(before);
    expect(send).not.toHaveBeenCalled();
  });
  it.each(['fragmentation-candidate', 'plan-conflict', 'regional-infeasible'])('links Activity to the exact %s decision without confusing it with the saved plan', async scene => {
    const rows = snapshot(scene), requested = String(rows['ui-decisions'][0].decision_id);
    const saved = String(rows['ui-placements'][0].decision_id), send = vi.fn(async () => {}), retry = vi.fn();
    await act(async () => root.render(<Lab views={mockViews(rows, scene, retry)}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry }}/>));
    await act(async () => element<HTMLElement>('.activity > summary').click());
    await act(async () => element<HTMLAnchorElement>(`.activity a[href="#decision-${requested}"]`).click());
    expect(element<HTMLElement>('#global-analysis').hidden).toBe(false);
    expect(element<HTMLDetailsElement>('.decision-details').open).toBe(true);
    expect(document.activeElement).toBe(document.getElementById(`decision-${requested}`));
    expect(document.activeElement?.scrollIntoView).toHaveBeenCalled();
    expect(document.getElementById(`decision-${requested}`)?.querySelector('.decision-context')?.textContent)
      .toBe(scene === 'regional-infeasible' ? 'Placement assessment · no new saved plan' : 'Separate placement attempt · not saved');
    expect(document.getElementById(`decision-${saved}`)?.querySelector('.decision-context')?.textContent).toBe(`Saved plan v${rows['ui-placements'][0].desired_plan_version}`);
    await act(async () => element<HTMLAnchorElement>('.analysis-content .evidence > a').click());
    expect(document.activeElement).toBe(document.getElementById(`decision-${saved}`));
    expect(send).not.toHaveBeenCalled();
    expect(retry).not.toHaveBeenCalled();
  });
  it('reports missing decision history instead of opening an unrelated decision', async () => {
    const rows = snapshot('healthy');
    rows['ui-decisions'] = [];
    await act(async () => root.render(<Lab views={mockViews(rows, 'healthy', () => {})}
      command={{ send: vi.fn(), pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    await act(async () => element<HTMLElement>('.activity > summary').click());
    await act(async () => element<HTMLAnchorElement>('.activity a').click());
    expect(element<HTMLDetailsElement>('.decision-details').open).toBe(true);
    expect(document.activeElement?.textContent).toBe('The requested decision is not in the available history.');
  });
  it('keeps expanded Activity up to date and marks disconnected history without stealing focus', async () => {
    const send = vi.fn(async () => {}), retry = vi.fn();
    const render = (scene: string, disconnected: boolean) => <Lab views={mockViews(snapshot(scene), scene, retry)}
      command={{ send, pending: false, notice: null, error: null }}
      connection={{ initialized: true, error: disconnected ? new Error('Connection lost') : null, retry }}/>;
    await act(async () => root.render(render('healthy', false)));
    await act(async () => element<HTMLElement>('.activity > summary').click());
    const workload = button('assistant');
    workload.focus();
    await act(async () => root.render(render('reports-expired', false)));
    expect(element<HTMLDetailsElement>('.activity').open).toBe(true);
    expect(element('.activity > summary').textContent).toContain('GPU reports overdue');
    expect(document.activeElement).toBe(workload);
    await act(async () => root.render(render('reports-expired', true)));
    expect(element('.activity > summary').textContent).toContain('Last received');
    expect(element('.activity .section-title').textContent).toContain('Out of date');
    expect(send).not.toHaveBeenCalled();
    expect(retry).not.toHaveBeenCalled();
  });
  it('shows empty/error Activity states and retries only when explicitly requested', async () => {
    const rows = snapshot('healthy'), retry = vi.fn(), send = vi.fn(async () => {});
    rows['ui-timeline'] = [];
    const views = mockViews(rows, 'healthy', retry);
    const render = () => <Lab views={views} command={{ send, pending: false, notice: null, error: null }}
      connection={{ initialized: true, error: null, retry: () => {} }}/>;
    await act(async () => root.render(render()));
    expect(element('.activity > summary').textContent).toContain('No activity recorded');
    views['ui-timeline'].error = new Error('Timeline unavailable');
    await act(async () => root.render(render()));
    expect(element('.activity > summary').textContent).toContain('Activity unavailable');
    await act(async () => element<HTMLElement>('.activity > summary').click());
    expect(element('.activity [role="alert"]').textContent).toContain('Timeline unavailable');
    expect(retry).not.toHaveBeenCalled();
    await act(async () => button('Retry updates', element('.activity')).click());
    expect(retry).toHaveBeenCalledOnce();
    expect(send).not.toHaveBeenCalled();
  });
  it.each([
    ['ready', ''], ['failed', 'Issue'], ['stopped', 'Issue'], ['starting', 'Starting'],
    ['policy-current', ''], ['plan-current', ''], ['resilience-current', ''], ['candidate-produced', ''],
    ['pending', 'Working'], ['policy-pending', 'Working'], ['awaiting-policy', 'Working'],
    ['future-status', 'Unknown'], ['empty', 'Unknown'], ['query-error', 'Unknown'], ['disconnected', 'Unknown'],
  ])('shows %s runtime status honestly in the header and read-only details', async (state, note) => {
    const rows = snapshot('healthy'), send = vi.fn(async () => {}), retry = vi.fn();
    rows['ui-status'][0].components = state === 'empty' ? [] : [{ component_id: 'placement-solver',
      status: ['query-error', 'disconnected'].includes(state) ? 'ready' : state, error: state === 'failed' ? 'Worker failed' : null }];
    const views = mockViews(rows, 'healthy', retry);
    if (state === 'query-error') views['ui-status'].error = new Error('Status unavailable');
    await act(async () => root.render(<Lab views={views} command={{ send, pending: false, notice: null, error: null }}
      connection={{ initialized: true, error: state === 'disconnected' ? new Error('Connection lost') : null, retry: () => {} }}/>));
    expect(button('System status').disabled).toBe(false);
    expect(button('System status').textContent).toBe(`System status${note ? ` · ${note}` : ''}`);
    await click('System status');
    expect(element('dialog').getAttribute('aria-label')).toBe('System status');
    if (state === 'failed') expect(element('dialog').textContent).toContain('Worker failed');
    if (state === 'query-error') {
      expect(element('dialog [role="alert"]').textContent).toContain('Status unavailable');
      await act(async () => button('Retry updates', element('dialog')).click());
      expect(retry).toHaveBeenCalledOnce();
    } else expect(retry).not.toHaveBeenCalled();
    if (state === 'disconnected') expect(element('dialog .component b').textContent).toContain('(last reported)');
    await click('Close system status');
    expect(document.querySelector('dialog')).toBeNull();
    expect(send).not.toHaveBeenCalled();
  });
  it('does not hide an unknown runtime state behind another component doing work', async () => {
    const rows = snapshot('healthy');
    rows['ui-status'][0].components = ['pending', 'future-status'].map((status, index) =>
      ({ component_id: `component-${index}`, status, error: null }));
    await act(async () => root.render(<Lab views={mockViews(rows, 'healthy', () => {})}
      command={{ send: vi.fn(async () => {}), pending: false, notice: null, error: null }}
      connection={{ initialized: true, error: null, retry: () => {} }}/>));
    expect(button('System status').textContent).toBe('System status · Unknown');
  });
  it('removes only the optional demo panel when showing the live interface, including open dialogs', async () => {
    const send = vi.fn(async () => {}), views = mockViews(snapshot('healthy'), 'healthy', () => {});
    const demoPanel = <section className="demo-panel">Demo-only controls and warnings</section>;
    const render = (showDemo: boolean) => <Lab views={views} demoPanel={showDemo ? demoPanel : undefined}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>;
    await act(async () => root.render(render(true)));
    await click('Add workload');
    expect(button('Save change').type).toBe('submit');
    const shared = element('.lab-app').cloneNode(true) as Element;
    shared.querySelector('.demo-panel')!.remove();
    await act(async () => root.render(render(false)));
    expect(element('.lab-app').innerHTML).toBe(shared.innerHTML);
    expect(document.querySelector('.demo-panel, .mode-label, #snapshot')).toBeNull();
    expect(element('#preset').closest('header')).not.toBeNull();
    expect(button('Reset scenario').closest('header')).not.toBeNull();
    expect(document.body.textContent).not.toMatch(/Load scenario|Apply to preview|Live updates connected/);
    expect(send).not.toHaveBeenCalled();
  });
  it('adopts the asynchronously observed starting scenario without overwriting a pending selection', async () => {
    const views = mockViews(snapshot('regional'), 'healthy', () => {});
    const status = views['ui-status'].data;
    views['ui-status'].data = [];
    const send = vi.fn(async () => {});
    const render = () => <Lab views={views} command={{ send, pending: false, notice: null, error: null }}
      connection={{ initialized: true, error: null, retry: () => {} }}/>;
    await act(async () => root.render(render()));
    views['ui-status'].data = status;
    await act(async () => root.render(render()));
    expect(element<HTMLSelectElement>('#preset').value).toBe('regional-boundary');
    await select('#preset', 'fragmentation');
    await act(async () => root.render(render()));
    expect(element<HTMLSelectElement>('#preset').value).toBe('fragmentation');
    expect(send).not.toHaveBeenCalled();
  });
  it.each(['baseline', 'fragmentation', 'regional-boundary'])('resets the live %s scenario only after confirmation, with no preview panel', async preset => {
    const send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(snapshot('healthy'), 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    expect(document.querySelector('.demo-panel, #snapshot')).toBeNull();
    expect([...document.querySelectorAll<HTMLOptionElement>('#preset option')].map(option => option.textContent))
      .toEqual(['Baseline fleet', 'Memory fragmentation', 'Regional policy']);
    await select('#preset', preset);
    expect(send).not.toHaveBeenCalled();
    vi.mocked(window.confirm).mockReturnValueOnce(false);
    await click('Reset scenario');
    expect(send).not.toHaveBeenCalled();
    await click('Reset scenario');
    expect(send).toHaveBeenCalledExactlyOnceWith(`/api/demo/presets/${preset}`, 'POST');
  });
  it.each([
    ['baseline', 'healthy'], ['fragmentation', 'fragmented'], ['regional-boundary', 'regional'],
  ])('loads the local %s fixture from the shared starting-scenario control in preview', async (preset, scene) => {
    await act(async () => root.render(<MockApp initialSnapshot="tenant-demand"/>));
    await select('#preset', preset);
    await click('Reset scenario');
    expect(element<HTMLSelectElement>('#snapshot').value).toBe(scene);
    expect(element<HTMLSelectElement>('#preset').value).toBe(preset);
    expect(document.querySelectorAll('.gpu')).toHaveLength(snapshot(scene)['ui-gpus'].length);
    expect(element('.demo-notice').textContent).toContain('No backend request was sent');
    expect(element('#preset').closest('.demo-panel')).toBeNull();
  });
  it.each(['feed-stale', 'query-error', 'bootstrap', 'pending'])('keeps recovery reset available while %s unless a command is pending', async state => {
    const send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(snapshot('healthy'), state === 'pending' ? 'healthy' : state, () => {})}
      command={{ send, pending: state === 'pending', notice: null, error: null }}
      connection={{ initialized: true, error: null, retry: () => {} }}/>));
    expect(button('Reset scenario').disabled).toBe(state === 'pending');
    await click('Reset scenario');
    if (state === 'pending') expect(send).not.toHaveBeenCalled();
    else expect(send).toHaveBeenCalledExactlyOnceWith('/api/demo/presets/baseline', 'POST');
  });
  it('permits corrective commands while current inputs are known but analysis is pending', async () => {
    const rows = snapshot('healthy');
    rows['ui-status'][0].scenario_ready = false;
    const send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(rows, 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }}
      connection={{ initialized: true, error: null, retry: () => {} }}/>));
    expect(button('Add workload').disabled).toBe(false);
    expect(button('Reset scenario').disabled).toBe(false);
    expect(send).not.toHaveBeenCalled();
  });
  it('shows a rejected live reset without changing the displayed fleet', async () => {
    let failure: string | null = null;
    const views = mockViews(snapshot('healthy'), 'healthy', () => {});
    const send = vi.fn(async () => {
      failure = 'Reset is unavailable until graph cleanup and readiness are integrated. No data was changed.';
      throw new Error(failure);
    });
    const render = () => <Lab views={views} command={{ send, pending: false, notice: null, error: failure }}
      connection={{ initialized: true, error: null, retry: () => {} }}/>;
    await act(async () => root.render(render()));
    const fleet = element('.fleet').textContent;
    await select('#preset', 'regional-boundary');
    await click('Reset scenario');
    await act(async () => root.render(render()));
    expect(element('.lab-app > .error').textContent).toContain('No data was changed');
    expect(element('.fleet').textContent).toBe(fleet);
    expect(document.querySelector('.notice, .demo-panel')).toBeNull();
  });
  it('keeps preview-only notices inside the demo panel while using the shared Save change control', async () => {
    await act(async () => root.render(<MockApp/>));
    await click('Add workload');
    expect(button('Save change').type).toBe('submit');
    element<HTMLInputElement>('input[name="name"]').value = 'preview-only';
    await submit();
    expect(element('.demo-notice').textContent).toContain('Preview updated in this tab only. No backend request was sent.');
    expect(element('.demo-notice').closest('.demo-panel')).not.toBeNull();
    expect(document.querySelector('.lab-app > .notice')).toBeNull();
    await click('Reload example state');
    expect(document.querySelector('.demo-notice')).toBeNull();
    expect(document.querySelector('.workload-summary')?.textContent).not.toContain('preview-only');
    await select('#snapshot', 'feed-stale');
    expect(element('.lab-app > .error').textContent).toContain('Updates are unavailable');
    await click('Reconnect updates');
    expect(element('.demo-notice').textContent).toContain('no connection is attempted');
  });
  it('retains real readiness errors and command notices without a demo panel', async () => {
    await act(async () => root.render(<Lab views={mockViews(snapshot('healthy'), 'feed-stale', () => {})}
      command={{ send: vi.fn(), pending: false, notice: 'Command receipt', error: 'Command failed' }}
      connection={{ initialized: true, error: new Error('Connection lost'), retry: () => {} }}/>));
    expect(document.querySelector('.demo-panel')).toBeNull();
    expect(element('.lab-app > .error').textContent).toContain('Connection lost');
    expect(element('.lab-app > .notice').textContent).toBe('Command receipt');
    expect(document.body.textContent).toContain('Command failed');
    expect(button('Add workload').disabled).toBe(true);
    expect(button('System status').textContent).toContain('Unknown');
  });
  it('keeps required counts per workload and shows regional activity even with the region collapsed', async () => {
    await act(async () => root.render(<MockApp initialSnapshot="regional"/>));
    expect([...document.querySelectorAll('.cluster-title .region-label')].map(node => node.textContent)).toEqual(['REGION', 'REGION', 'REGION']);
    expect([...document.querySelectorAll('h3')].some(node => node.textContent === 'VMs and GPUs')).toBe(false);
    for (const heading of document.querySelectorAll('.region-heading')) expect(heading.firstElementChild?.textContent).toBe('REGION');
    expect([...document.querySelectorAll('.regional-replicas')].map(node => node.textContent?.trim())).toEqual([
      'Replicas: 8 planned · 8 running · 8 confirmed', 'Replicas: 0 planned · 0 running · 0 confirmed', 'Replicas: 0 planned · 0 running · 0 confirmed',
    ]);
    expect([...document.querySelectorAll('thead th')].map(node => node.textContent)).toContain('Replicas required');
    expect([...document.querySelectorAll('thead th')].map(node => node.textContent)).toContain('Confirmed');
    expect([...document.querySelectorAll('thead th')].map(node => node.textContent)).not.toContain('Ready');
    for (const row of document.querySelectorAll('tbody tr')) {
      expect([row.children[2].textContent, row.children[3].textContent, row.children[4].textContent]).toEqual(['2', '2', '2']);
    }
    const cluster = element<HTMLDetailsElement>('.cluster-body');
    expect(cluster.open).toBe(false);
    expect(element('.regional-replicas').closest('summary')).not.toBeNull();
    await click('assistant');
    expect(element('.regional-replicas').textContent).toContain('8 planned · 8 running · 8 confirmed');
    await select('#snapshot', 'region-recovered');
    expect([...document.querySelectorAll('.regional-replicas')].map(node => node.textContent?.trim())).toEqual([
      'Replicas: 0 planned · 0 running · 0 confirmed', 'Replicas: 8 planned · 8 running · 8 confirmed', 'Replicas: 0 planned · 0 running · 0 confirmed',
    ]);
    await select('#snapshot', 'regional-infeasible');
    expect([...document.querySelectorAll('.regional-replicas')].map(node => node.textContent?.trim())).toEqual([
      'Replicas: 0 planned · 0 running · 0 confirmed', 'Replicas: 8 planned · 4 running · 4 confirmed', 'Replicas: 0 planned · 0 running · 0 confirmed',
    ]);
  });
  it('starts with one thin, selectable Workloads summary above all regions and expands the existing table', async () => {
    loadStyles();
    const send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(snapshot('regional'), 'regional', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    expect(document.querySelectorAll('.workload-panel')).toHaveLength(1);
    expect(document.querySelectorAll('.cluster')).toHaveLength(3);
    expect(element('.lab-layout').firstElementChild).toBe(element('.workload-panel'));
    const disclosure = element<HTMLDetailsElement>('.workload-body'), summary = element<HTMLElement>('.workload-body > summary');
    expect(disclosure.open).toBe(false);
    expect(summary.getAttribute('aria-controls')).toBe('workload-details');
    expect(getComputedStyle(element('#workload-details')).display).toBe('none');
    const header = element('.workload-heading'), headerMarkup = header.innerHTML;
    expect(summary.closest('.workload-heading')).toBeNull();
    expect(element('.workload-summary').closest('.workload-heading')).toBeNull();
    expect(header.nextElementSibling).toBe(element('.workload-overview'));
    expect(getComputedStyle(header).padding).toBe(getComputedStyle(element('.cluster-title')).padding);
    expect(getComputedStyle(header).backgroundColor).toBe(getComputedStyle(element('.cluster-title')).backgroundColor);
    expect(getComputedStyle(element('.workload-summary')).overflowX).toBe('auto');
    expect(document.querySelectorAll('.workload-summary .workload-chip')).toHaveLength(4);
    expect([...document.querySelectorAll('.workload-chip > span[id]')].map(node => node.textContent)).toEqual(Array(4).fill('2 / 2 confirmed'));
    expect(element<HTMLElement>('.workload-progress > span').style.width).toBe('100%');
    await act(async () => button('assistant', element('.workload-summary')).click());
    expect(disclosure.open).toBe(false);
    expect(button('assistant', element('.workload-summary')).closest('summary')).toBeNull();
    await act(async () => element<HTMLElement>('.cluster-body > summary').click());
    await click('chat · replica 1');
    expect(button('chat', element('.workload-summary')).getAttribute('aria-pressed')).toBe('true');
    expect(disclosure.open).toBe(false);
    await toggleWorkloads();
    expect(header.innerHTML).toBe(headerMarkup);
    expect(disclosure.open).toBe(true);
    expect(getComputedStyle(element('#workload-details')).display).not.toBe('none');
    expect(getComputedStyle(element('.workload-summary')).display).toBe('none');
    expect(button('chat', element('#workload-details')).getAttribute('aria-pressed')).toBe('true');
    await act(async () => button('assistant', element('#workload-details')).click());
    await toggleWorkloads();
    expect(disclosure.open).toBe(false);
    expect(header.innerHTML).toBe(headerMarkup);
    expect(button('assistant', element('.workload-summary')).getAttribute('aria-pressed')).toBe('true');
    expect(document.querySelectorAll('.selected-replica')).toHaveLength(2);
    expect(send).not.toHaveBeenCalled();
  });
  it.each([
    ['healthy', 'healthy', ''], ['reports-paused', 'healthy', 'Reports paused'],
    ['reports-expired', 'unreachable', 'Reports paused'], ['power-off', 'healthy', 'Power off'],
    ['feed-stale', 'unknown', ''], ['bootstrap', 'unknown', ''],
  ])('shows a compact %s VM/GPU summary below an unchanged region header', async (scene, health, setting) => {
    loadStyles();
    await act(async () => root.render(<MockApp initialSnapshot={scene}/>));
    const region = element('.cluster'), header = element('.cluster-title'), headerMarkup = header.innerHTML;
    const preview = element('.region-preview');
    expect(region.querySelector<HTMLDetailsElement>('.cluster-body')?.open).toBe(false);
    expect(header.innerHTML).toBe(headerMarkup);
    expect(getComputedStyle(preview).display).toBe('flex');
    expect(getComputedStyle(preview).overflowX).toBe('auto');
    expect(preview.querySelectorAll('.vm-preview')).toHaveLength(3);
    expect(preview.querySelectorAll('.mini-gpu')).toHaveLength(6);
    expect(preview.textContent).toContain('GPU reports');
    const worker = preview.querySelector('.vm-preview')!;
    expect(worker.querySelectorAll(`.report-${health}`)).toHaveLength(2);
    if (setting) {
      expect(worker.textContent).toContain(setting);
      expect(worker.querySelector('.mini-gpu')?.getAttribute('aria-label')).toContain(setting);
    }
    if (health === 'unknown') expect(preview.querySelector('.report-healthy')).toBeNull();
    await act(async () => region.querySelector<HTMLElement>('.cluster-body > summary')!.click());
    expect(getComputedStyle(preview).display).toBe('none');
    expect(header.innerHTML).toBe(headerMarkup);
    await act(async () => region.querySelector<HTMLElement>('.cluster-body > summary')!.click());
    expect(getComputedStyle(preview).display).toBe('flex');
    expect(header.innerHTML).toBe(headerMarkup);
  });
  it('shows only registered devices and an explicit empty region in collapsed summaries', async () => {
    const rows = snapshot('regional');
    rows['ui-gpus'] = rows['ui-gpus'].filter(g => g.gpu_id !== 'mock-inference-a-1' && g.cluster_id !== 'us-spare');
    const send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(rows, 'regional', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    expect(document.querySelector('.cluster-body[open]')).toBeNull();
    const previews = document.querySelectorAll('.region-preview');
    expect(previews).toHaveLength(3);
    expect(previews[0].querySelectorAll('.mini-gpu')).toHaveLength(5);
    expect(previews[0].querySelector('.vm-preview')?.querySelectorAll('.mini-gpu')).toHaveLength(1);
    expect(previews[1].querySelectorAll('.mini-gpu')).toHaveLength(4);
    expect(previews[2].querySelectorAll('.vm-preview, .mini-gpu')).toHaveLength(0);
    expect(previews[2].textContent).toContain('No VMs registered');
    expect(button('Add ready VM to US Spare').disabled).toBe(false);
    expect(send).not.toHaveBeenCalled();
  });
  it('updates a collapsed region preview without reopening it or treating a disconnect as healthy', async () => {
    const render = (disconnected: boolean) => <Lab views={mockViews(snapshot('healthy'), 'healthy', () => {})}
      command={{ send: vi.fn(), pending: false, notice: null, error: null }}
      connection={{ initialized: true, error: disconnected ? new Error('Connection lost') : null, retry: () => {} }}/>;
    await act(async () => root.render(render(false)));
    expect(document.querySelectorAll('.region-preview .report-healthy')).toHaveLength(6);
    await act(async () => root.render(render(true)));
    expect(element<HTMLDetailsElement>('.cluster-body').open).toBe(false);
    expect(document.querySelectorAll('.region-preview .report-healthy')).toHaveLength(0);
    expect(document.querySelectorAll('.region-preview .report-unknown')).toHaveLength(6);
  });
  it.each(['reports-expired', 'feed-stale', 'bootstrap'])('shows honest %s replica counts in the collapsed Workloads summary', async scene => {
    await act(async () => root.render(<MockApp initialSnapshot={scene}/>));
    const chip = button('assistant', element('.workload-summary'));
    expect(chip.textContent).toContain(scene === 'reports-expired' ? '1 / 2 confirmed' : 'Unknown / 2 confirmed');
    expect(chip.classList.contains('workload-confirmed')).toBe(false);
    expect(chip.title).toContain(scene === 'reports-expired' ? '2 running, 1 confirmed' : 'Unknown running, Unknown confirmed');
    expect(chip.querySelector<HTMLElement>('.workload-progress > span')?.style.width).toBe(scene === 'reports-expired' ? '50%' : '0%');
    await toggleWorkloads();
    const row = element('tbody tr');
    expect(row.children[4].textContent).toBe(scene === 'reports-expired' ? '1' : 'Unknown');
  });
  it('updates collapsed summary counts without expanding it or stealing selection', async () => {
    const render = (scene: string) => <Lab views={mockViews(snapshot(scene), scene, () => {})}
      command={{ send: vi.fn(), pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>;
    await act(async () => root.render(render('healthy')));
    await click('assistant');
    await act(async () => root.render(render('reports-expired')));
    expect(element<HTMLDetailsElement>('.workload-body').open).toBe(false);
    expect(button('assistant', element('.workload-summary')).textContent).toContain('1 / 2 confirmed');
    expect(button('assistant', element('.workload-summary')).getAttribute('aria-pressed')).toBe('true');
  });
  it('retains user-expanded panels across data updates without expanding other regions', async () => {
    const render = (scene: string) => <Lab views={mockViews(snapshot(scene), scene, () => {})}
      command={{ send: vi.fn(), pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>;
    await act(async () => root.render(render('regional')));
    await toggleWorkloads();
    await act(async () => element<HTMLElement>('.cluster-body > summary').click());
    await click('Expand GPU details');
    await act(async () => root.render(render('regional-infeasible')));
    expect(element<HTMLDetailsElement>('.workload-body').open).toBe(true);
    expect([...document.querySelectorAll<HTMLDetailsElement>('.cluster-body')].map(panel => panel.open)).toEqual([true, false, false]);
    expect(document.querySelectorAll('.gpu-expanded')).toHaveLength(1);
    expect(element<HTMLDetailsElement>('.activity').open).toBe(false);
  });
  it('handles an empty workload list and creates a workload directly from its collapsed header', async () => {
    const rows = snapshot('healthy'), send = vi.fn(async () => {});
    rows['ui-workloads'] = [];
    await act(async () => root.render(<Lab views={mockViews(rows, 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    expect(element('.workload-summary').textContent).toContain('No workloads configured');
    await click('Add workload');
    expect(element<HTMLDetailsElement>('.workload-body').open).toBe(false);
    element<HTMLInputElement>('input[name="name"]').value = 'new-assistant';
    await submit();
    expect(send).toHaveBeenCalledExactlyOnceWith('/api/workloads', 'POST', {
      name: 'new-assistant', profile_id: 'assistant-v1', replicas: 2, data_profile_id: 'demo-open', purpose: 'demo', spread_across_domains: true,
    }, expect.any(String));
  });
  it('opens policy editing from the collapsed Workloads header without expanding or changing the fleet', async () => {
    const send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(snapshot('regional'), 'regional', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    const summary = element('.workload-summary').textContent;
    await click('Edit policy');
    expect(element('#dialog-title').textContent).toBe('Edit processing policy');
    expect(element<HTMLDetailsElement>('.workload-body').open).toBe(false);
    expect(element('.workload-summary').textContent).toBe(summary);
    await click('Close dialog');
    expect(document.querySelector('dialog')).toBeNull();
    expect(send).not.toHaveBeenCalled();
  });
  it.each([
    ['EU Primary', 'eu-primary'], ['EU Recovery', 'eu-recovery'], ['US Spare', 'us-spare'],
  ])('creates a ready VM in %s without falling back to the first region', async (name, id) => {
    const send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(snapshot('regional'), 'regional', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    expect(document.querySelectorAll('.cluster-title button[aria-label^="Add ready VM to"]')).toHaveLength(3);
    const region = button(`Add ready VM to ${name}`).closest('.cluster')!;
    expect(region.querySelector<HTMLDetailsElement>('.cluster-body')?.open).toBe(false);
    await click(`Add ready VM to ${name}`);
    expect(element('.host-region').textContent).toContain(name);
    expect(document.querySelector('dialog select[name="cluster"]')).toBeNull();
    element<HTMLInputElement>('input[name="name"]').value = 'added-vm';
    await submit();
    expect(send).toHaveBeenCalledExactlyOnceWith('/api/hosts', 'POST', {
      host_id: 'added-vm', cluster_id: id, hardware_profile_id: 'h100-nvl-pair-v1',
    }, expect.any(String));
  });
  it('allows adding a VM to an empty region and blocks an open form if that region disappears', async () => {
    const rows = snapshot('regional'), send = vi.fn(async () => {});
    rows['ui-gpus'] = rows['ui-gpus'].filter(g => g.cluster_id !== 'us-spare');
    Object.assign(rows['ui-clusters'].find(c => c.cluster_id === 'us-spare')!, { registered_workers: 0, healthy_gpus: 0 });
    const render = () => <Lab views={mockViews(rows, 'regional', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>;
    await act(async () => root.render(render()));
    expect(button('Add ready VM to US Spare').disabled).toBe(false);
    await click('Add ready VM to US Spare');
    element<HTMLInputElement>('input[name="name"]').value = 'us-new';
    rows['ui-clusters'] = rows['ui-clusters'].filter(c => c.cluster_id !== 'us-spare');
    await act(async () => root.render(render()));
    expect(element('dialog [role="alert"]').textContent).toContain('selected region is no longer available');
    expect(element<HTMLButtonElement>('dialog button[type="submit"]').disabled).toBe(true);
    await submit();
    expect(send).not.toHaveBeenCalled();
  });
  it('keeps inspection available while disabling relocated creation buttons on stale data', async () => {
    await act(async () => root.render(<MockApp initialSnapshot="feed-stale"/>));
    expect(button('Add workload').disabled).toBe(true);
    expect(button('Add ready VM').disabled).toBe(true);
    await toggleWorkloads();
    expect(element<HTMLDetailsElement>('.workload-body').open).toBe(true);
    expect(button('assistant', element('.workload-summary')).disabled).toBe(false);
  });
  it('does not show old regional counts as current when the connection alone fails', async () => {
    await act(async () => root.render(<Lab views={mockViews(snapshot('healthy'), 'healthy', () => {})}
      command={{ send: vi.fn(async () => {}), pending: false, notice: null, error: null }}
      connection={{ initialized: true, error: new Error('Connection interrupted'), retry: () => {} }}/>));
    expect(element('.regional-replicas').textContent).toContain('Unknown planned · Unknown running · Unknown confirmed');
  });
  it('distinguishes configured demand from the saved regional target until a new plan is saved', async () => {
    await act(async () => root.render(<MockApp initialSnapshot="fragmentation-candidate"/>));
    const required = () => [...document.querySelectorAll('tbody tr')].reduce((sum, row) => sum + Number(row.children[2].textContent), 0);
    expect(required()).toBe(7);
    expect(element('.regional-replicas').textContent).toContain('6 planned · 6 running · 6 confirmed');
    expect(element<HTMLElement>('.planned-replicas').title).toContain('saved plan v1');
    expect(element<HTMLElement>('.planned-replicas').title).toContain('lag behind configuration changes');
    await select('#snapshot', 'plan-committed');
    expect(required()).toBe(7);
    expect(element('.regional-replicas').textContent).toContain('7 planned · 6 running · 0 confirmed');
    expect(element<HTMLElement>('.planned-replicas').title).toContain('saved plan v2');
  });
  it('links workload selection to its GPU replicas in both directions without sending commands', async () => {
    const send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(snapshot('healthy'), 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    const before = element('.regional-replicas').textContent;
    await click('assistant');
    expect(element('.selected-workload').textContent).toContain('assistant');
    expect([...document.querySelectorAll('.selected-replica .replica-link')].map(node => node.textContent)).toEqual([
      'assistant · replica 1', 'assistant · replica 2',
    ]);
    expect(document.querySelectorAll('.replica-link[aria-pressed=true]')).toHaveLength(2);
    await click('chat · replica 1');
    expect(button('assistant').getAttribute('aria-pressed')).toBe('false');
    expect(button('chat').getAttribute('aria-pressed')).toBe('true');
    expect([...document.querySelectorAll('.selected-replica .replica-link')].map(node => node.textContent)).toEqual([
      'chat · replica 1', 'chat · replica 2',
    ]);
    await click('Expand GPU details');
    expect(document.querySelectorAll('.selected-replica')).toHaveLength(2);
    await click('chat · replica 2');
    expect(document.querySelector('.selected-replica, .selected-workload')).toBeNull();
    await click('embeddings');
    expect(document.querySelectorAll('.selected-replica')).toHaveLength(2);
    expect(button('embeddings').getAttribute('aria-pressed')).toBe('true');
    await click('embeddings');
    expect(document.querySelector('.selected-replica, .selected-workload')).toBeNull();
    expect(element('.regional-replicas').textContent).toBe(before);
    expect(send).not.toHaveBeenCalled();
  });
  it('links a planned replica to its workload without implying that it is running', async () => {
    await act(async () => root.render(<MockApp initialSnapshot="plan-committed"/>));
    await click('assistant · replica 1');
    const tile = element('.selected-replica');
    expect(tile.classList.contains('desired')).toBe(true);
    expect(tile.textContent).toContain('Planned here · not confirmed running');
    const row = element('.selected-workload');
    expect([row.children[2].textContent, row.children[3].textContent, row.children[4].textContent]).toEqual(['1', '0', '0']);
    expect(element('.regional-replicas').textContent).toContain('7 planned · 6 running · 0 confirmed');
  });
  it('uses fixed header slots for policy indicators without inserting text rows when replicas are selected', async () => {
    loadStyles();
    await act(async () => root.render(<MockApp initialSnapshot="regional"/>));
    const cards = [...document.querySelectorAll('.gpu')];
    const content = cards.map(card => card.textContent);
    const children = cards.map(card => [...card.children]);
    const slots = [...document.querySelectorAll<HTMLElement>('.gpu-title .policy-indicator')];
    expect(slots).toHaveLength(14);
    for (const slot of slots) {
      const css = getComputedStyle(slot);
      expect(css.width).toBe('20px'); expect(css.height).toBe('20px'); expect(css.flexShrink).toBe('0');
      expect(css.visibility).toBe('hidden');
      expect(slot.getAttribute('aria-hidden')).toBe('true');
    }
    await click('assistant · replica 1');
    expect(document.querySelector('.eligibility-label')).toBeNull();
    expect(cards.map(card => card.textContent)).toEqual(content);
    cards.forEach((card, i) => expect([...card.children]).toEqual(children[i]));
    expect(document.querySelectorAll('.policy-indicator.policy-allow')).toHaveLength(10);
    expect(document.querySelectorAll('.policy-indicator.policy-deny')).toHaveLength(4);
    for (const slot of slots) {
      const css = getComputedStyle(slot);
      expect(css.width).toBe('20px'); expect(css.height).toBe('20px'); expect(css.flexShrink).toBe('0');
      expect(css.visibility).toBe('visible');
      expect(slot.getAttribute('aria-hidden')).toBe('false');
      expect(slot.tagName).toBe('BUTTON');
      expect(slot.getAttribute('aria-haspopup')).toBe('dialog');
      expect(slot.getAttribute('aria-label')).toBe(`Policy details: ${slot.title}`);
      expect(slot.title).toBe(slot.classList.contains('policy-allow') ? 'assistant · policy: Allowed' : 'assistant · policy: Not allowed');
    }
    await click('assistant · replica 1');
    expect(document.querySelector('.selected-replica')).toBeNull();
    expect(cards.map(card => card.textContent)).toEqual(content);
    for (const slot of slots) {
      expect(getComputedStyle(slot).width).toBe('20px');
      expect(getComputedStyle(slot).visibility).toBe('hidden');
      expect(slot.hasAttribute('aria-label')).toBe(false);
    }
  });
  it.each(['policy-unknown', 'feed-stale', 'query-error'])('shows unknown rather than an allowed indicator for %s', async scene => {
    await act(async () => root.render(<MockApp initialSnapshot={scene}/>));
    await click('assistant · replica 1');
    expect(document.querySelector('.policy-allow, .policy-deny')).toBeNull();
    const slots = [...document.querySelectorAll<HTMLElement>('.policy-indicator')];
    expect(slots.length).toBeGreaterThan(0);
    for (const slot of slots) {
      expect(slot.classList.contains('policy-unknown')).toBe(true);
      expect(slot.title).toBe('assistant · policy: Unknown');
    }
    expect(document.querySelector('.eligibility-label')).toBeNull();
  });
  it('opens only the selected workload and region policy details without changing the fleet', async () => {
    const send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(snapshot('regional'), 'regional', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    expect(element<HTMLButtonElement>('.policy-indicator').disabled).toBe(true);
    expect(document.querySelector('dialog, .eligibility')).toBeNull();
    await click('assistant · replica 1');
    const indicator = element<HTMLButtonElement>('.policy-indicator.policy-deny');
    expect(indicator.disabled).toBe(false);
    indicator.focus();
    expect(document.activeElement).toBe(indicator);
    await act(async () => indicator.click());
    const dialog = element<HTMLDialogElement>('dialog');
    expect(dialog.open).toBe(true);
    expect(dialog.getAttribute('aria-label')).toBe('Policy details');
    expect(dialog.querySelectorAll('.eligibility')).toHaveLength(1);
    expect(dialog.querySelector('.eligibility > strong')?.textContent).toBe('assistant → US Spare');
    expect(dialog.textContent).toContain('Not allowed');
    expect(dialog.querySelector('form, input, select')).toBeNull();
    expect(dialog.querySelector('pre')?.textContent).toContain('mock-assistant/us-spare');
    expect(document.querySelectorAll('.selected-replica')).toHaveLength(2);
    await click('Close policy details');
    expect(document.querySelector('dialog')).toBeNull();
    await act(async () => indicator.click());
    await act(async () => { element('dialog').dispatchEvent(new Event('cancel', { bubbles: true, cancelable: true })); });
    expect(document.querySelector('dialog')).toBeNull();
    expect(send).not.toHaveBeenCalled();
  });
  it.each(['policy-unknown', 'query-error', 'connection-error', 'missing-pair'])('updates an open policy inspector safely when %s arrives', async scene => {
    const send = vi.fn(async () => {}), rows = snapshot('regional');
    const render = (changed: boolean) => {
      const next = changed && scene === 'policy-unknown' ? snapshot(scene) : structuredClone(rows);
      if (changed && scene === 'missing-pair') next['ui-policy'] = next['ui-policy'].filter(p => p.id !== 'mock-assistant/eu-primary');
      return <Lab views={mockViews(next, changed && scene === 'query-error' ? 'query-error' : 'regional', () => {})}
        command={{ send, pending: false, notice: null, error: null }}
        connection={{ initialized: true, error: changed && scene === 'connection-error' ? new Error('Connection lost') : null, retry: () => {} }}/>;
    };
    await act(async () => root.render(render(false)));
    await click('assistant');
    await click('Policy details: assistant · policy: Allowed');
    expect(element('dialog .eligibility .badge').textContent).toBe('Allowed');
    await act(async () => root.render(render(true)));
    const dialog = element<HTMLDialogElement>('dialog');
    expect(dialog.open).toBe(true);
    expect(dialog.querySelector('.eligibility .badge.good')).toBeNull();
    expect(dialog.textContent).toContain(scene === 'missing-pair' ? 'No current data to display' : 'Unknown / not current');
    if (scene === 'query-error') expect(dialog.querySelector('[role="alert"]')?.textContent).toContain('policy updates are unavailable');
    expect(send).not.toHaveBeenCalled();
  });
  it.each(['policy-unknown', 'fencing-pending', 'policy-fenced', 'feed-stale'])('keeps %s replica relationships inspectable without changing evidence', async scene => {
    await act(async () => root.render(<MockApp initialSnapshot={scene}/>));
    const tile = element('.allocation'), before = tile.textContent;
    const link = element<HTMLButtonElement>('.replica-link');
    expect(link.disabled).toBe(false);
    expect(link.type).toBe('button');
    expect(link.tabIndex).toBe(0);
    link.focus();
    expect(document.activeElement).toBe(link);
    await act(async () => link.click());
    expect(link.getAttribute('aria-pressed')).toBe('true');
    expect(tile.textContent).toBe(before);
    expect(element('.selected-workload').textContent).toContain('assistant');
    if (scene === 'feed-stale') {
      expect(button('Pause reports').disabled).toBe(true);
      expect(document.querySelectorAll('.gpu .badge.good')).toHaveLength(0);
    }
  });
  it('keeps an orphaned historical replica readable rather than linking to a missing workload', async () => {
    const rows = snapshot('healthy');
    rows['ui-workloads'] = rows['ui-workloads'].filter(w => w.workload_id !== 'mock-assistant');
    await act(async () => root.render(<Lab views={mockViews(rows, 'healthy', () => {})}
      command={{ send: vi.fn(async () => {}), pending: false, notice: null, error: null }}
      connection={{ initialized: true, error: null, retry: () => {} }}/>));
    expect(element('.allocation strong').textContent).toBe('mock-assistant · replica 1');
    expect(element('.allocation strong').querySelector('button')).toBeNull();
  });
  it('labels GPU VMs plainly and keeps their GPU counts accurate', async () => {
    await act(async () => root.render(<MockApp/>));
    const headings = [...document.querySelectorAll('.worker h4')].map(h => h.textContent?.trim());
    expect(headings).toEqual(['inference-a', 'inference-b', 'inference-c'].map(host => `${host} · GPU VM · 2 GPUs`));
    expect(document.body.textContent).not.toContain('worker failure domain');
    await click('Remove GPU');
    expect(element('.worker h4').textContent?.trim()).toBe('inference-a · GPU VM · 1 GPU');
  });
  it('starts with condensed GPU cards and expands only the selected GPU', async () => {
    await act(async () => root.render(<MockApp/>));
    expect(document.querySelectorAll('.gpu-condensed')).toHaveLength(6);
    expect(document.querySelectorAll('.gpu-expanded')).toHaveLength(0);
    expect(document.querySelectorAll('.gpu-details[hidden]')).toHaveLength(6);
    expect(document.querySelectorAll('.allocation small')).toHaveLength(0);
    const gpu = element('.gpu');
    expect(gpu.textContent).toContain('76 / 80 GiB');
    expect(gpu.textContent).toContain('65 / 85 units');
    expect(gpu.textContent).toContain('assistant · replica 1');
    expect(gpu.textContent).toContain('Running');
    expect(gpu.textContent).not.toContain('Report interval');
    const expand = button('Expand GPU details', gpu);
    expect(expand.type).toBe('button');
    expect(expand.tabIndex).toBe(0);
    expect(expand.getAttribute('aria-expanded')).toBe('false');
    const details = document.getElementById(expand.getAttribute('aria-controls')!);
    expect(details?.hidden).toBe(true);
    expand.focus();
    expect(document.activeElement).toBe(expand);
    await act(async () => expand.click());
    expect(document.activeElement).toBe(button('Collapse GPU details', gpu));
    expect(expand.getAttribute('aria-expanded')).toBe('true');
    expect(details?.hidden).toBe(false);
    expect(document.querySelectorAll('.gpu-expanded')).toHaveLength(1);
    expect(document.querySelectorAll('.gpu-condensed')).toHaveLength(5);
    expect(gpu.textContent).toContain('Report interval: 1000 ms');
    expect(gpu.textContent).toContain('Requirements: 76 GiB · 55 demand units');
    expect(gpu.textContent).toContain('Simulator confirmed this action');
    await act(async () => button('Collapse GPU details', gpu).click());
    expect(details?.hidden).toBe(true);
    expect(gpu.textContent).not.toContain('Report interval');
    expect(gpu.textContent).toContain('assistant · replica 1');
  });
  it('gives every GPU icon an action name and hover text without visible button text', async () => {
    await act(async () => root.render(<MockApp/>));
    const gpu = element('.gpu');
    for (const name of ['Expand GPU details', 'Pause reports', 'Simulate GPU failure', 'Edit background load', 'Exclude GPU from plans', 'Remove GPU']) {
      const action = button(name, gpu);
      expect(action.title).toBe(name);
      expect(action.getAttribute('aria-label')).toBe(name);
      expect(action.textContent).toBe('');
      expect(action.querySelector('svg')?.getAttribute('aria-hidden')).toBe('true');
      expect(action.disabled).toBe(false);
    }
    expect(gpu.querySelector('.gpu-controls')?.getAttribute('aria-label')).toBe('Actions for inference-a / GPU 0');
  });
  it.each([
    ['Pause reports', 'reporting_enabled', true, false, '/telemetry'],
    ['Resume reports', 'reporting_enabled', false, true, '/telemetry'],
    ['Simulate GPU failure', 'powered_on', true, false, '/telemetry'],
    ['Restore GPU', 'powered_on', false, true, '/telemetry'],
    ['Exclude GPU from plans', 'scheduling_enabled', true, false, ''],
    ['Include GPU in plans', 'scheduling_enabled', false, true, ''],
  ] as const)('preserves the %s command and its revision', async (name, field, before, after, suffix) => {
    const rows = snapshot('healthy'), send = vi.fn(async () => {});
    Object.assign(rows['ui-gpus'][0], { [field]: before, inventory_revision: '12', telemetry_revision: '34' });
    await act(async () => root.render(<Lab views={mockViews(rows, 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    await click(name);
    expect(send).toHaveBeenCalledExactlyOnceWith(`/api/gpus/mock-inference-a-0${suffix}`, 'PATCH', {
      expected_revision: suffix ? '34' : '12', changes: { [field]: after },
    });
  });
  it('opens background settings from the condensed card and preserves the edit command', async () => {
    const rows = snapshot('healthy'), send = vi.fn(async () => {});
    rows['ui-gpus'][0].telemetry_revision = '34';
    await act(async () => root.render(<Lab views={mockViews(rows, 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    await click('Edit background load');
    expect(element<HTMLInputElement>('input[name="demand"]').value).toBe('10');
    element<HTMLInputElement>('input[name="demand"]').value = '15';
    element<HTMLInputElement>('input[name="memory"]').value = '1024';
    await submit();
    expect(send).toHaveBeenCalledExactlyOnceWith('/api/gpus/mock-inference-a-0/telemetry', 'PATCH', {
      expected_revision: '34', changes: { background_compute_units: 15, background_memory_mib: 1024, interval_ms: 1000 },
    }, undefined);
    expect(document.querySelector('dialog')).toBeNull();
    expect(document.querySelectorAll('.gpu-condensed')).toHaveLength(6);
  });
  it('still requires confirmation to remove a GPU and sends its inventory revision', async () => {
    const rows = snapshot('healthy'), send = vi.fn(async () => {});
    rows['ui-gpus'][0].inventory_revision = '12';
    await act(async () => root.render(<Lab views={mockViews(rows, 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    vi.mocked(window.confirm).mockReturnValueOnce(false);
    await click('Remove GPU');
    expect(window.confirm).toHaveBeenCalledWith('Remove inference-a / GPU 0 from inventory? Any workload replicas on this simulated GPU will stop.');
    expect(send).not.toHaveBeenCalled();
    await click('Remove GPU');
    expect(send).toHaveBeenCalledExactlyOnceWith('/api/gpus/mock-inference-a-0', 'DELETE', undefined, undefined, '12');
  });
  it.each(['feed-stale', 'query-error', 'pending'] as const)('blocks icon actions during %s but still permits inspecting details', async state => {
    const send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(snapshot('healthy'), state === 'pending' ? 'healthy' : state, () => {})}
      command={{ send, pending: state === 'pending', notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    for (const action of document.querySelectorAll<HTMLButtonElement>('.gpu-controls button')) {
      expect(action.disabled).toBe(true);
      await act(async () => action.click());
    }
    expect(send).not.toHaveBeenCalled();
    expect(button('Expand GPU details').disabled).toBe(false);
    await click('Expand GPU details');
    expect(element('.gpu-details').hasAttribute('hidden')).toBe(false);
  });
  it.each([
    ['reports-paused', 'Reports paused'],
    ['reports-expired', 'Report overdue'],
    ['power-off', 'Power setting: off'],
    ['policy-unknown', 'Paused by policy'],
    ['fencing-pending', 'Stop requested · not yet confirmed'],
    ['policy-fenced', 'Stopped by policy'],
    ['plan-committed', 'Planned here · not confirmed running'],
    ['feed-stale', 'Replica activity (last received data)'],
  ] as const)('keeps %s evidence visible without expanding a GPU', async (scene, expected) => {
    await act(async () => root.render(<MockApp initialSnapshot={scene}/>));
    expect([...document.querySelectorAll('.gpu-condensed')].some(gpu => gpu.textContent?.includes(expected))).toBe(true);
    expect(document.querySelectorAll('.gpu-expanded')).toHaveLength(0);
  });
  it('shows changed settings and unknown measurements without inventing current replica activity', async () => {
    await act(async () => root.render(<MockApp/>));
    await click('Pause reports');
    const gpu = element('.gpu');
    expect(button('Resume reports', gpu).title).toBe('Resume reports');
    expect(gpu.textContent).toContain('Reports paused');
    expect(gpu.textContent).toContain('Replica activity unknown');
    expect(gpu.querySelectorAll('.gauge meter')).toHaveLength(0);
    expect(gpu.querySelectorAll('.gauge .meter-unknown')).toHaveLength(2);
    await click('Simulate GPU failure');
    expect(button('Restore GPU', gpu).title).toBe('Restore GPU');
    expect(gpu.textContent).toContain('Power setting: off');
    await click('Exclude GPU from plans');
    expect(button('Include GPU in plans', gpu).title).toBe('Include GPU in plans');
    expect(gpu.textContent).toContain('Excluded from plans');
  });
  it('switches snapshots, selects workload eligibility and keeps healthy US devices distinct from denials', async () => {
    await act(async () => root.render(<MockApp/>));
    await select('#snapshot', 'regional');
    await click('assistant');
    expect(document.querySelectorAll('.destination-deny')).toHaveLength(4);
    expect(document.querySelectorAll('.destination-allow')).toHaveLength(10);
    expect(element('.destination-deny .badge').textContent).toBe('Recent report');
    expect(document.querySelector('.eligibility')).toBeNull();
    await select('#snapshot', 'reports-expired');
    expect(element('.regional-replicas').textContent).toContain('8 planned · 8 running · 6 confirmed');
    await select('#snapshot', 'plan-committed');
    expect(document.querySelectorAll('.allocation.desired')).toHaveLength(2);
    expect(document.body.textContent).toContain('Saved; waiting to apply');
    await select('#snapshot', 'plan-confirmed');
    expect(document.querySelectorAll('.allocation.desired')).toHaveLength(0);
    expect(element('.regional-replicas').textContent).toContain('7 planned · 7 running · 7 confirmed');
  });
  it('prefills current policy parameters and sends only the deliberate edit', async () => {
    await act(async () => root.render(<MockApp initialSnapshot="regional"/>));
    await click('Edit policy');
    expect(element<HTMLInputElement>('input[value="westeurope"]').checked).toBe(true);
    expect(element<HTMLInputElement>('input[value="northeurope"]').checked).toBe(true);
    expect(element<HTMLInputElement>('input[value="eastus"]').checked).toBe(false);
    await act(async () => element<HTMLInputElement>('input[value="westeurope"]').click());
    await submit();
    expect(document.querySelector('dialog')).toBeNull();
    expect(document.body.textContent).toContain('Existing'); // Historical decisions remain inspectable.
    await click('assistant');
    await click('Policy details: assistant · policy: Unknown');
    expect(element('dialog').textContent).toContain('Allowed regions: North Europe');
    expect(element('dialog').textContent).toContain('Unknown / not current');
    await click('Close policy details');
    expect(element('.regional-replicas').textContent).toContain('Unknown planned · Unknown running · Unknown confirmed');
    await click('Reload example state');
    expect(element('.regional-replicas').textContent).toContain('8 planned · 8 running · 8 confirmed');
  });
  it('preserves permissive wildcard policy on a no-op form submission', async () => {
    await act(async () => root.render(<MockApp/>));
    await click('Edit policy');
    expect(element<HTMLInputElement>('input[value="*"]').checked).toBe(true);
    expect(element<HTMLInputElement>('input[value="eastus"]').disabled).toBe(true);
    await submit();
    await click('assistant');
    await click('Policy details: assistant · policy: Unknown');
    expect(element('dialog').textContent).toContain('Allowed regions: All regions');
  });
  it('supports adding a ready VM locally without preserving stale measurements', async () => {
    await act(async () => root.render(<MockApp/>));
    await click('Add ready VM');
    element<HTMLInputElement>('input[name="name"]').value = 'inference-d';
    await submit();
    expect(document.querySelectorAll('.gpu')).toHaveLength(8);
    expect(document.body.textContent).toContain('inference-d / GPU 0');
    expect(document.body.textContent).toContain('Replica activity unknown');
    expect(element('.regional-replicas').textContent).toContain('Unknown planned · Unknown running · Unknown confirmed');
  });
  it('disables commands on stale feeds and supports returning to a valid snapshot', async () => {
    await act(async () => root.render(<MockApp initialSnapshot="feed-stale"/>));
    expect(button('Add ready VM').disabled).toBe(true);
    expect(button('Edit policy').disabled).toBe(true);
    expect(element('.regional-replicas').textContent).toContain('Unknown planned · Unknown running · Unknown confirmed');
    expect(document.querySelectorAll('.gpu .badge.good')).toHaveLength(0);
    await click('Reconnect updates');
    expect(document.body.textContent).toContain('no connection is attempted');
    await select('#snapshot', 'healthy');
    expect(button('Add ready VM').disabled).toBe(false);
  });
  it('disables an already-open dialog when query readiness is lost', async () => {
    const rows = snapshot('healthy'), send = vi.fn(async () => {});
    const render = (stale: boolean) => <Lab views={mockViews(rows, stale ? 'feed-stale' : 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>;
    await act(async () => root.render(render(false)));
    await click('Add workload');
    await act(async () => root.render(render(true)));
    expect(element<HTMLButtonElement>('dialog button[type="submit"]').disabled).toBe(true);
    await submit();
    expect(send).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain('Actions are unavailable');
  });
  it.each(scenarios)('$id uses plain-language copy outside technical details', async scene => {
    await act(async () => root.render(<MockApp initialSnapshot={scene.id}/>));
    await click('Expand GPU details');
    const content = document.createElement('div');
    content.innerHTML = document.body.innerHTML;
    content.querySelectorAll('.technical-details, pre').forEach(node => node.remove());
    expect(content.textContent?.match(/\b(workers?|fixtures?|fenced|fencing|suspended|MILP|telemetry|heartbeat|epoch|signatures?|idempotency|acknowledgement)\b/gi) ?? []).toEqual([]);
    expect(content.textContent).toContain('How to read this demo');
    expect(content.textContent).toContain('Demand units are not utilization percentages');
    expect(content.textContent).toContain('GPU VM');
    expect(element<HTMLSelectElement>('#snapshot').value).toBe(scene.id);
  });
  it('keeps API names and raw codes unchanged behind VM and policy labels', async () => {
    const rows = snapshot('healthy'), send = vi.fn(async () => {});
    await act(async () => root.render(<Lab views={mockViews(rows, 'healthy', () => {})}
      command={{ send, pending: false, notice: null, error: null }} connection={{ initialized: true, error: null, retry: () => {} }}/>));
    await click('Power off VM');
    expect(send).toHaveBeenCalledWith('/api/hosts/inference-a/telemetry', 'PATCH', {
      gpu_revisions: { 'mock-inference-a-0': '1', 'mock-inference-a-1': '1' }, changes: { powered_on: false },
    });
    await act(async () => root.render(<MockApp initialSnapshot="policy-fenced"/>));
    expect(element('.allocation.fenced').textContent).toContain('Stopped by policy');
    expect([...document.querySelectorAll('.technical-details pre')].some(node => node.textContent?.includes('"state": "fenced"'))).toBe(true);
  });
});
