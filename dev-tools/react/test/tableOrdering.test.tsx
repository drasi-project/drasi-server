// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DataTable } from '../src/components';
import { compareTableValues } from '../src/components/DataTable';

interface Row { id: string; value: number | string | null | undefined }

function permutations<T>(values: readonly T[]): T[][] {
  if (!values.length) return [[]];
  return values.flatMap((value, index) =>
    permutations(values.filter((_, other) => index !== other)).map(rest => [value, ...rest]),
  );
}

const ids = () => screen.getAllByRole('row').slice(1)
  .map(row => within(row).getAllByRole('cell')[0].textContent);

describe('coherent table ordering', () => {
  it('is reflexive, antisymmetric and transitive across numeric, textual and nullish groups', () => {
    const values: unknown[] = [
      -Infinity, -10, -2, -0, 0, 2, 10, Infinity, NaN,
      '', '-11', '2', '10', '11', 'A', '\u00c4', 'A\u0308', 'Z',
      false, true, 10n, Symbol('reading'), {}, null, undefined,
    ];
    for (const a of values) {
      expect(compareTableValues(a, a)).toBe(0);
      for (const b of values) {
        const ab = compareTableValues(a, b);
        expect(Number.isFinite(ab)).toBe(true);
        expect(Math.sign(ab) + Math.sign(compareTableValues(b, a))).toBe(0);
        for (const c of values) {
          if (ab <= 0 && compareTableValues(b, c) <= 0) {
            expect(compareTableValues(a, c)).toBeLessThanOrEqual(0);
          }
        }
      }
    }
  });

  it('orders numeric edge cases without accidental NaN comparisons or numeric-string coercion', () => {
    const values = [NaN, Infinity, 10, 2, 0, -0, -10, -Infinity, '2', '11', '10', undefined, null];
    const sorted = [...values].sort(compareTableValues);
    // Array.sort itself moves undefined to the end; DataTable compares row objects instead.
    expect(sorted.slice(0, 8)).toEqual([-Infinity, -10, 0, -0, 2, 10, Infinity, NaN]);
    expect(sorted.slice(8)).toEqual(['10', '11', '2', null, undefined]);
    expect(compareTableValues(NaN, NaN)).toBe(0);
    expect(compareTableValues(Infinity, Infinity)).toBe(0);
    expect(compareTableValues(-0, 0)).toBe(0);
    expect(compareTableValues(null, undefined)).toBe(0);
  });

  it.each([
    { values: [2, 10, '11'], ordered: ['0', '1', '2'] },
    { values: [-2, -10, '-11'], ordered: ['1', '0', '2'] },
    { values: [3, 20, '100'], ordered: ['0', '1', '2'] },
  ])('sorts every permutation of $values consistently, in both directions', ({ values, ordered }) => {
    const rows: Row[] = values.map((value, index) => ({ id: String(index), value }));
    const table = render(<DataTable rows={rows} columns={[{ key: 'id', label: 'ID' }, { key: 'value', label: 'Value' }]} rowKey={row => row.id} />);
    for (const direction of ['asc', 'desc'] as const) {
      for (const permutation of permutations(rows)) {
        table.rerender(<DataTable
          rows={Object.freeze(permutation)}
          columns={[{ key: 'id', label: 'ID' }, { key: 'value', label: 'Value' }]}
          rowKey={row => row.id}
          sort={{ column: 'value', direction }}
        />);
        expect(ids()).toEqual(direction === 'asc' ? ordered : [...ordered].reverse());
        expect(screen.getByRole('columnheader', { name: 'Value' }).getAttribute('aria-sort'))
          .toBe(direction === 'asc' ? 'ascending' : 'descending');
      }
    }
  });

  it('honors explicit left without overriding omitted inherited alignment', () => {
    render(<div style={{ textAlign: 'center' }}><DataTable
      rows={[{ id: 'rack', value: 'reading' }]}
      columns={[{ key: 'id', label: 'Explicit', align: 'left' }, { key: 'value', label: 'Inherited' }]}
      rowKey={row => row.id}
    /></div>);
    const cells = screen.getAllByRole('cell');
    expect(cells[0].classList.contains('drasi-align--left')).toBe(true);
    expect(cells[1].classList.contains('drasi-align--left')).toBe(false);
    expect(screen.getByRole('columnheader', { name: 'Explicit' }).classList.contains('drasi-align--left')).toBe(true);
  });

  it('preserves stable numeric/NaN/nullish ties and keyed row nodes while reversing only ordering', () => {
    const rows: Row[] = [
      { id: 'plus-zero', value: 0 }, { id: 'minus-zero', value: -0 },
      { id: 'nan-a', value: NaN }, { id: 'nan-b', value: NaN },
      { id: 'missing', value: undefined }, { id: 'null', value: null },
    ];
    const props = { rows, columns: [{ key: 'id', label: 'ID' }, { key: 'value', label: 'Value' }], rowKey: (row: Row) => row.id };
    const table = render(<DataTable {...props} sort={{ column: 'value', direction: 'asc' }} />);
    expect(ids()).toEqual(['plus-zero', 'minus-zero', 'nan-a', 'nan-b', 'missing', 'null']);
    const original = screen.getByText('plus-zero').closest('tr');
    table.rerender(<DataTable {...props} sort={{ column: 'value', direction: 'desc' }} />);
    expect(ids()).toEqual(['missing', 'null', 'nan-a', 'nan-b', 'plus-zero', 'minus-zero']);
    expect(screen.getByText('plus-zero').closest('tr')).toBe(original);
  });

  it('keeps actual Trading name collation instead of replacing it with ordinal ordering', () => {
    const names = ['NVIDIA Corporation', 'Nike Inc.', 'AbbVie Inc.', 'Abbott Laboratories'];
    expect([...names].sort(compareTableValues)).toEqual([
      'Abbott Laboratories', 'AbbVie Inc.', 'Nike Inc.', 'NVIDIA Corporation',
    ]);
    expect([...names].sort(compareTableValues))
      .toEqual([...names].sort((a, b) => a.localeCompare(b, 'en-US')));
  });
});
