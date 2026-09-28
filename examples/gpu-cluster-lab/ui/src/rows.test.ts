import { describe, it, expect } from 'vitest';
import { rowKey, validateRow } from './rows';
import { fixture } from './mock/fixtures';

describe('CQ row identity and validation', () => {
  it('accepts sparse deletes without requiring projected fields', () => {
    expect(rowKey('ui-gpus', { gpu_id: 'device-1' })).toBe('device-1');
    expect(rowKey('ui-policy', { id: 'workload/cluster' })).toBe('workload/cluster');
  });
  it('rejects missing identities instead of manufacturing keys', () => {
    expect(() => rowKey('ui-gpus', { name: 'device' })).toThrow();
    expect(() => rowKey('ui-gpus', { gpu_id: 1 })).toThrow();
  });
  it('validates after-images, not sparse before-images', () => {
    expect(() => validateRow('ui-gpus', { gpu_id: 'device-1' })).toThrow();
    expect(validateRow('ui-clusters', { cluster_id: 'eu', name: 'Europe', region: 'westeurope', registered_workers: 0, healthy_gpus: null })).toEqual({
      cluster_id: 'eu', name: 'Europe', region: 'westeurope', registered_workers: 0, healthy_gpus: null,
    });
  });
  it('preserves an unknown policy pair without inventing a policy revision', () => {
    const row = { ...fixture('baseline')['ui-policy'][0], policy_revision: null,
      authorization: 'unknown', current: false, error: 'Policy record missing', allowed_regions: [] };
    expect(validateRow('ui-policy', row)).toEqual(row);
    expect(() => validateRow('ui-policy', { ...row, authorization: 'allow', current: true, error: null })).toThrow('policy metadata');
  });
});
