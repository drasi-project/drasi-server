// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { describe, expect, it } from 'vitest';
import { nativeQueries } from '../../scripts/export-native-recipe';
import { ALL_QUERIES, PORTFOLIO_QUERY } from '../../src/services/queries';

describe('native Trading recipe export', () => {
  it('retains all current query inputs, joins and projections in order', () => {
    const exported = nativeQueries(ALL_QUERIES);
    expect(exported).toHaveLength(11);
    for (const [index, query] of exported.entries()) {
      const { description: _description, ...definition } = ALL_QUERIES[index];
      expect(query).toEqual({ ...definition, queryLanguage: 'Cypher', autoStart: true });
      expect(query.inputs.length).toBeGreaterThan(0);
      expect(query).not.toHaveProperty('description');
      expect(query).not.toHaveProperty('sources');
      expect(JSON.parse(JSON.stringify(query)).inputs).toEqual(definition.inputs);
    }
  });

  it('preserves result labels rather than silently dropping query settings', () => {
    const definition = { ...PORTFOLIO_QUERY, result: { label: 'PortfolioPosition' } };
    expect(nativeQueries([definition])[0]).toMatchObject({
      inputs: ['postgres-stocks', 'price-feed'],
      joins: PORTFOLIO_QUERY.joins,
      result: { label: 'PortfolioPosition' },
    });
    expect(definition.description).toBe(PORTFOLIO_QUERY.description);
  });
});
