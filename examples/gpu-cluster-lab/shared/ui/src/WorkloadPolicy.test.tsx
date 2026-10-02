// @vitest-environment jsdom
import { act, StrictMode } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { Lab } from './App';
import { mockViews } from './mock/MockApp';
import { snapshot } from './mock/scenarios';
import { policyMetadata } from './WorkloadPolicy';
import { recordMap, text, validateRow } from './rows';

let root: Root;
const send = vi.fn(async () => {});
const connection = { initialized: true, error: null, retry: vi.fn() };
function element<T extends Element>(selector: string): T {
  const found = document.querySelector<T>(selector);
  if (!found) throw new Error(`Missing ${selector}`);
  return found;
}
async function click(selector: string) {
  await act(async () => element<HTMLElement>(selector).click());
}
async function select(selector: string, value: string) {
  await act(async () => {
    const input = element<HTMLSelectElement>(selector);
    input.value = value;
    input.dispatchEvent(new Event('change', { bubbles: true }));
  });
}
const assistant = '.workload-summary [aria-label="Policy results for assistant"]';
beforeEach(() => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
  vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('Policy views must not request data'));
  vi.spyOn(window, 'confirm').mockReturnValue(true);
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value() {
    this.open = true; this.querySelector('button')?.focus();
  } });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', { configurable: true, value() { this.open = false; } });
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: vi.fn() });
  const host = document.createElement('div'); document.body.append(host); root = createRoot(host);
  send.mockClear(); connection.retry.mockClear();
});
afterEach(async () => {
  await act(async () => root.unmount());
  expect(fetch).not.toHaveBeenCalled();
  expect(connection.retry).not.toHaveBeenCalled();
  vi.restoreAllMocks(); vi.unstubAllGlobals(); document.body.replaceChildren();
});
function render(rows = snapshot('regional'), stale = false, disconnected = false) {
  return <StrictMode><Lab views={mockViews(rows, stale ? 'feed-stale' : 'regional', connection.retry)}
    connection={{ ...connection, initialized: !disconnected }}
    command={{ send, pending: false, notice: null, error: null }}/></StrictMode>;
}

it('keeps shared rules in a closed side panel, separate from workload results and without a policy name', async () => {
  await act(async () => root.render(render()));
  expect(element<HTMLDetailsElement>('.workload-body').open).toBe(false);
  const summaries = [...document.querySelectorAll('.workload-summary .workload-policy-summary')];
  expect(summaries).toHaveLength(4);
  for (const summary of summaries) {
    expect(summary.textContent).toContain('Policy results');
    expect(summary.textContent).not.toContain('Customer processing policy');
    expect(summary.textContent).toContain('Allowed: West Europe, North Europe');
    expect(summary.textContent).toContain('Blocked: East US');
  }
  const rules = element<HTMLElement>('#data-policy');
  expect(rules.hidden).toBe(true);
  expect(element('.lab-content').firstElementChild).toBe(element('.workload-panel'));
  await click('[aria-controls="data-policy"]');
  expect(rules.hidden).toBe(false);
  expect(rules.querySelector('h2')?.textContent).toBe('Data policy');
  expect(rules.querySelector('h4')).toBeNull();
  expect(rules.querySelector('details')?.textContent).toContain('Example customer EU documents');
  expect(rules.textContent).toContain('Shared rules govern all workloads');
  expect(rules.textContent).toContain('Customer must matchcustomer-eu');
  expect(rules.textContent).toContain('Classification must beRestricted');
  expect(rules.textContent).toContain('Purpose must beCustomer support');
  expect(rules.textContent).toContain('assistant, chat, embeddings, reranker');
  expect(document.querySelector('.workload-chip[aria-pressed="true"]')).toBeNull();
  expect(send).not.toHaveBeenCalled();
});

it('separates classification, purpose and policy results without changing workload facts', async () => {
  await act(async () => root.render(render()));
  const headers = [...document.querySelectorAll('#workload-details th')].map(header => header.textContent);
  expect(headers.slice(6, 10)).toEqual(['Data Classification', 'Purpose', 'Policy results', 'Actions']);
  const row = element('.workload-row');
  expect(row.children[6].textContent).toContain('Example customer EU documents');
  expect(row.children[6].textContent).toContain('Restricted');
  expect(row.children[6].textContent).not.toContain('Purpose');
  expect(row.children[6].querySelector('.workload-policy-summary')).toBeNull();
  expect(row.children[7].textContent).toBe('Customer support');
  expect(row.children[8].textContent).toContain('Policy results');
  await click('.workload-row .replica-expander');
  expect(element<HTMLTableCellElement>('.replica-detail-row td').colSpan).toBe(headers.length);
  expect(send).not.toHaveBeenCalled();
});

it('connects data, purpose, shared policy and per-workload allocation in a read-only inspector', async () => {
  await act(async () => root.render(render()));
  const trigger = element<HTMLButtonElement>(assistant);
  trigger.focus();
  await click(assistant);
  const dialog = element('dialog');
  expect(dialog.textContent).toContain('Policy & allocation: assistant');
  expect(dialog.textContent).toContain('Workload facts, shared rules, destination results');
  expect(dialog.textContent).toContain('Example customer EU documents');
  expect(dialog.textContent).toContain('Customer support');
  expect(dialog.textContent).toContain('customer-eu-processing');
  expect(dialog.textContent).toContain('Data classificationRestricted');
  expect(dialog.textContent).not.toContain('Edit this policy');
  expect(dialog.textContent).toContain('Allowed does not mean allocated');
  expect(dialog.querySelectorAll('.eligibility')).toHaveLength(3);
  expect(dialog.querySelector('.policy-allocation-counts')?.textContent).toContain('2 in saved plan · 2 running · 2 confirmed');
  expect(dialog.textContent).toContain('Excluded from new allocations');
  await act(async () => dialog.dispatchEvent(new Event('cancel', { cancelable: true })));
  expect(document.querySelector('dialog')).toBeNull();
  expect(document.activeElement).toBe(trigger);
  expect(send).not.toHaveBeenCalled();
});

it.each(['stale', 'disconnected', 'purpose', 'epoch', 'signature', 'rules-changing', 'missing'])('does not present %s policy decisions as allowed', async state => {
  const rows = snapshot('regional');
  if (state === 'purpose') rows['ui-workloads'][0].purpose = 'demo';
  if (state === 'epoch') rows['ui-policy'].forEach(row => { row.observation_epoch = 'previous-run'; });
  if (state === 'signature') rows['ui-policy'].forEach(row => { row.policy_signature = 'previous-policy'; });
  if (state === 'rules-changing') rows['ui-status'][0].policy_rules_current = false;
  if (state === 'missing') rows['ui-policy'] = rows['ui-policy'].filter(row => row.workload_id !== 'mock-assistant');
  await act(async () => root.render(render(rows, state === 'stale', state === 'disconnected')));
  const summary = element(assistant).closest('.workload-policy-summary')!;
  expect(summary.querySelector('.good')).toBeNull();
  expect(summary.querySelector('.warning')).not.toBeNull();
  await click(assistant);
  expect(document.querySelector('dialog .eligibility .badge.good')).toBeNull();
  if (state === 'missing') expect(element('dialog').textContent).toContain('EU Primary / West Europe: policy result unavailable');
  expect(send).not.toHaveBeenCalled();
});

it('explains purpose denial even when the destination appears in the configured region list', async () => {
  const rows = snapshot('regional');
  rows['ui-workloads'][0].purpose = 'demo';
  rows['ui-policy'].filter(row => row.workload_id === 'mock-assistant').forEach(row => {
    row.purpose = 'demo'; row.authorization = 'deny'; row.reasons = ['purpose-not-permitted'];
  });
  await act(async () => root.render(render(rows)));
  expect(element(assistant).closest('.workload-policy-summary')?.textContent).not.toContain('Allowed:');
  await click(assistant);
  expect(element('dialog').textContent).toContain('This processing purpose is not allowed by the data policy');
  expect(document.querySelector('dialog .badge.good')).toBeNull();
  expect(send).not.toHaveBeenCalled();
});

it('edits a shared data scope, names affected workloads and sends only the region edit', async () => {
  const rows = snapshot('regional');
  const baseline = snapshot('healthy')['ui-status'][0];
  for (const [field, identity] of [['policy_rules', 'policy_id'], ['data_profiles', 'data_profile_id']]) {
    rows['ui-status'][0][field] = Object.fromEntries([...recordMap(rows['ui-status'][0], field, identity)!,
      ...recordMap(baseline, field, identity)!].map(row => [text(row, identity), row]));
  }
  const chat = rows['ui-workloads'].find(row => row.name === 'chat')!;
  chat.data_profile_id = 'demo-open'; chat.purpose = 'demo';
  rows['ui-policy'].filter(row => row.workload_id === chat.workload_id).forEach(row => Object.assign(row, {
    data_profile_id: 'demo-open', purpose: 'demo', policy_id: 'demo-permissive',
    allowed_regions: ['*'], authorization: 'allow', reasons: [], authority_ref: 'demo-fixture',
  }));
  await act(async () => root.render(render(rows)));
  await click('[aria-controls="data-policy"]');
  const trigger = [...document.querySelectorAll<HTMLButtonElement>('.shared-rule button')][1];
  trigger.focus();
  await act(async () => trigger.click());
  expect(document.querySelectorAll('dialog')).toHaveLength(1);
  expect(element<HTMLSelectElement>('select[name="policy"]').value).toBe('demo-permissive');
  expect(element('.policy-edit-scope').textContent).toContain('apply to: chat.');
  expect(element('.policy-edit-scope').textContent).not.toContain('assistant');
  expect(send).not.toHaveBeenCalled();
  await act(async () => element('form').dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
  expect(send).toHaveBeenCalledExactlyOnceWith('/api/policies/demo-permissive', 'PATCH', {
    expected_revision: '1', changes: { allowed_regions: ['*'] },
  }, undefined);
  expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining('for chat?'));
  expect(document.activeElement).toBe(trigger);
});

it.each(['changed', 'removed'])('does not silently overwrite or replace a shared rule %s while its editor was open', async state => {
  const rows = snapshot('regional');
  const status = rows['ui-status'][0];
  status.policy_rules = { 'customer-eu-processing': recordMap(status, 'policy_rules', 'policy_id')!.find(row => row.policy_id === 'customer-eu-processing')! };
  status.data_profiles = { 'customer-eu-documents': recordMap(status, 'data_profiles', 'data_profile_id')!.find(row => row.data_profile_id === 'customer-eu-documents')! };
  await act(async () => root.render(render(rows)));
  await click('[aria-controls="data-policy"]');
  await click('.edit-data-policy');
  expect(document.querySelector('dialog select[name="policy"]')).toBeNull();
  expect(element('.policy-edit-scope').textContent).not.toContain('Example customer EU documents');
  if (state === 'removed') {
    rows['ui-status'][0].policy_rules = {}; rows['ui-status'][0].data_profiles = {};
  } else {
    const rule = recordMap(rows['ui-status'][0], 'policy_rules', 'policy_id')![0];
    Object.assign(rule, { revision: '2', allowed_regions: ['northeurope'] });
    rows['ui-status'][0].policy_rules = { [text(rule, 'policy_id')]: rule };
  }
  await act(async () => root.render(render(rows)));
  expect(document.querySelector('dialog select[name="policy"]')).toBeNull();
  await act(async () => element('form').dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
  expect(element('dialog [role="alert"]').textContent).toContain(state === 'removed'
    ? 'Current policy revision is unavailable' : 'This policy changed while the editor was open');
  expect(send).not.toHaveBeenCalled();
});

it('shares the analysis drawer slot, returns focus, and preserves the open panel behind modal editing', async () => {
  const rows = snapshot('regional');
  await act(async () => root.render(render(rows)));
  const toggle = element<HTMLButtonElement>('[aria-controls="data-policy"]');
  await click('[aria-controls="global-analysis"]');
  await click('[aria-controls="data-policy"]');
  expect(element<HTMLElement>('#global-analysis').hidden).toBe(true);
  expect(element<HTMLElement>('#data-policy').hidden).toBe(false);
  expect(toggle.getAttribute('aria-expanded')).toBe('true');
  expect(document.activeElement).toBe(element('[aria-label="Close data policy"]'));
  const workload = element<HTMLButtonElement>('.workload-chip');
  workload.focus();
  await act(async () => root.render(render(rows, true)));
  expect(document.activeElement).toBe(workload);
  expect(element('#data-policy .warning').textContent).toContain('matching current evaluation');
  await act(async () => root.render(render(rows)));
  element<HTMLButtonElement>('.edit-data-policy').focus();
  await click('.edit-data-policy');
  await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); });
  expect(element<HTMLElement>('#data-policy').hidden).toBe(false);
  await act(async () => element('dialog').dispatchEvent(new Event('cancel', { cancelable: true })));
  expect(document.activeElement).toBe(element('.edit-data-policy'));
  await act(async () => { document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); });
  expect(element<HTMLElement>('#data-policy').hidden).toBe(true);
  expect(document.activeElement).toBe(toggle);
  await click('[aria-controls="data-policy"]');
  await click('[aria-controls="global-analysis"]');
  expect(element<HTMLElement>('#data-policy').hidden).toBe(true);
  expect(element<HTMLElement>('#global-analysis').hidden).toBe(false);
  expect(send).not.toHaveBeenCalled();
});

it('edits workload facts without a policy selector or authorizing unsaved changes', async () => {
  await act(async () => root.render(render()));
  await click('.workload-actions button:last-child');
  expect(element<HTMLSelectElement>('select[name="data"]').value).toBe('customer-eu-documents');
  expect(element<HTMLSelectElement>('select[name="purpose"]').value).toBe('customer-support');
  expect(element('.workload-policy-binding').textContent).toContain('Data classification: Restricted');
  expect(document.querySelector('dialog select[name="policy"]')).toBeNull();
  expect([...element<HTMLSelectElement>('select[name="data"]').options].map(option => option.value)).toEqual(['customer-eu-documents', 'demo-open']);
  await select('select[name="purpose"]', 'demo');
  expect(element('.workload-policy-binding').textContent).toContain('Purpose: Demo');
  expect(element('.workload-policy-binding').textContent).toContain('does not preview a new permission decision');
  expect(send).not.toHaveBeenCalled();
});

it('changes the displayed classification with the selected data, not the purpose, and saves the selected facts', async () => {
  await act(async () => root.render(render()));
  await click('.workload-body > summary');
  await click('.workload-row td:last-child button:first-child');
  expect(element('.workload-policy-binding').textContent).toContain('Data classification: Restricted');
  await select('select[name="purpose"]', 'demo');
  expect(element('.workload-policy-binding').textContent).toContain('Data classification: Restricted');
  await select('select[name="data"]', 'demo-open');
  expect(element('.workload-policy-binding').textContent).toContain('Data classification: Synthetic');
  expect(element<HTMLSelectElement>('select[name="purpose"]').value).toBe('demo');
  expect(element('.workload-policy-binding').textContent).toContain('does not preview a new permission decision');
  expect(send).not.toHaveBeenCalled();
  await act(async () => element('form').dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
  expect(send).toHaveBeenCalledOnce();
  expect(send.mock.calls[0]).toEqual([
    expect.stringMatching(/^\/api\/workloads\//), 'PATCH',
    expect.objectContaining({ expected_revision: '1', changes: expect.objectContaining({ data_profile_id: 'demo-open', purpose: 'demo' }) }), undefined,
  ]);
});

it('reopens a saved synthetic workload as Synthetic and does not silently change its purpose', async () => {
  const rows = snapshot('regional');
  rows['ui-workloads'][0].data_profile_id = 'demo-open';
  await act(async () => root.render(render(rows)));
  await click('.workload-body > summary');
  await click('.workload-row td:last-child button:first-child');
  expect(element<HTMLSelectElement>('select[name="data"]').value).toBe('demo-open');
  expect(element<HTMLSelectElement>('select[name="purpose"]').value).toBe('customer-support');
  expect(element('.workload-policy-binding').textContent).toContain('Data classification: Synthetic');
  await select('select[name="data"]', 'customer-eu-documents');
  expect(element('.workload-policy-binding').textContent).toContain('Data classification: Restricted');
  expect(element<HTMLSelectElement>('select[name="purpose"]').value).toBe('customer-support');
  expect(send).not.toHaveBeenCalled();
  await act(async () => element('dialog').dispatchEvent(new Event('cancel', { cancelable: true })));
  expect(document.activeElement).toBe(element('.workload-row td:last-child button:first-child'));
});

it('returns from results to the shared rules with focus and without a command', async () => {
  await act(async () => root.render(render()));
  await click(assistant);
  await click('.workload-policy-context button');
  await act(async () => { await new Promise(resolve => requestAnimationFrame(resolve)); });
  expect(document.querySelector('dialog')).toBeNull();
  expect(document.activeElement).toBe(element('#data-policy'));
  expect(send).not.toHaveBeenCalled();
});

it.each(['legacy', 'missing', 'pending'])('does not invent %s rule criteria or classify data from its name', async state => {
  const rows = snapshot('regional');
  const status = rows['ui-status'][0];
  if (state === 'legacy') for (const field of ['policy_rules', 'data_profiles', 'policy_rules_current']) delete status[field];
  else if (state === 'missing') Object.assign(status, { policy_rules: null, data_profiles: null, policy_rules_current: false });
  else status.policy_rules_current = false;
  validateRow('ui-status', status);
  await act(async () => root.render(render(rows)));
  expect(element('#data-policy .warning').textContent).toContain(state === 'pending' ? 'matching current evaluation' : 'Full data-policy criteria are unavailable');
  if (state !== 'pending') expect(element('.workload-row').children[6].textContent).toContain('Classification unavailable');
  expect(send).not.toHaveBeenCalled();
});

it.each(['identity', 'criteria', 'profile', 'incomplete'])('rejects invalid %s in source-backed rule payloads', invalid => {
  const status = snapshot('regional')['ui-status'][0];
  const rules = recordMap(status, 'policy_rules', 'policy_id')!;
  if (invalid === 'identity') status.policy_rules = { wrong: rules[0] };
  if (invalid === 'criteria') status.policy_rules = { 'customer-eu-processing': { ...rules[0], allowed_purposes: 'customer-support' } };
  if (invalid === 'profile') status.data_profiles = { 'customer-eu-documents': {
    ...recordMap(status, 'data_profiles', 'data_profile_id')![0], policy_id: 'missing',
  } };
  if (invalid === 'incomplete') status.data_profiles = null;
  expect(() => validateRow('ui-status', status)).toThrow();
});

it('does not guess a policy definition while its query rows disagree', () => {
  const rows = snapshot('regional')['ui-policy'];
  expect(policyMetadata(rows)?.policy_id).toBe('customer-eu-processing');
  const changed = { ...rows[0], policy_revision: '2' };
  expect(policyMetadata([rows[0], changed])).toBeUndefined();
  expect(policyMetadata([{ ...rows[0], policy_id: 'unresolved' }])).toBeUndefined();
});
