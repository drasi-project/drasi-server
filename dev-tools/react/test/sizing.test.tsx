// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { render } from '@testing-library/react';
import { expect, it } from 'vitest';
import { DataTable } from '../src/components/DataTable';
import { tableHeight, type TableHeight } from '../src/components/sizing';

it.each<[TableHeight, string]>([
  [0, '0px'], [320, '320px'], [12.5, '12.5px'], ['0', '0'], ['320px', '320px'],
  ['20rem', '20rem'], ['50%', '50%'], ['75dvh', '75dvh'], ['10ch', '10ch'],
  ['auto', 'auto'], ['var(--drasi-table-height)', 'var(--drasi-table-height)'],
  ['var(--inventory-height, 25rem)', 'var(--inventory-height, 25rem)'],
  ['var(--inventory-height, auto)', 'var(--inventory-height, auto)'],
])('normalizes explicit size %s without class scanning', (value, expected) => {
  expect(tableHeight(value)).toBe(expected);
});

it.each([
  -1, Infinity, NaN, 'h-[400px]', '400', '-5rem', '', 'inherit', 'red',
  'calc(100vh - 2rem)', 'var(height)', 'var(--height, red)', 'var(--height, -1px)',
])('rejects invalid JS height %s rather than silently ignoring it', value => {
  expect(() => Reflect.apply(tableHeight, undefined, [value])).toThrow(TypeError);
});

it('uses one sizing rule for loaded and state cards and gives explicit height precedence', () => {
  const props = { columns: [{ key: 'id', label: 'Id' }], rowKey: (row: { id: string }) => row.id };
  const table = render(<DataTable {...props} rows={[]} height={280} style={{ height: '500px', width: '50%' }} />);
  const card = () => table.container.querySelector<HTMLElement>('.drasi-query-table');
  expect(card()?.style.height).toBe('280px');
  expect(card()?.style.width).toBe('50%');
  table.rerender(<DataTable {...props} rows={null} state={{ loading: true }} height="18rem" />);
  expect(card()?.style.height).toBe('18rem');
  table.rerender(<DataTable {...props} rows={null} state={{ error: new Error('Offline') }} height={0} />);
  expect(card()?.style.height).toBe('0px');
  table.rerender(<DataTable {...props} rows={[]} style={{ height: '500px' }} />);
  expect(card()?.style.height).toBe('500px');
  table.rerender(<DataTable {...props} rows={[]} />);
  expect(card()?.style.height).toBe('');
});
