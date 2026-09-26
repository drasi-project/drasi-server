// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { StrictMode } from 'react';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import { DataTable } from '../src/components/DataTable';

const props = {
  rows: [{ id: 'one', value: 2 }, { id: 'two', value: 1 }],
  columns: [{ key: 'value', label: 'Value' }],
  rowKey: (row: { id: string }) => row.id,
};

it('applies a CSS length to the table card rather than treating it as a class', () => {
  const { container } = render(<DataTable {...props} height="260px" />);
  const card = container.querySelector<HTMLElement>('.drasi-query-table');
  expect(card?.style.height).toBe('260px');
  expect(card?.classList.contains('260px')).toBe(false);
});

it('sorts through a native button without making a column header an interactive element', async () => {
  const change = vi.fn();
  const user = userEvent.setup();
  render(<StrictMode><DataTable {...props} onSortChange={change} /></StrictMode>);
  const heading = screen.getByRole('columnheader', { name: 'Value' });
  const button = within(heading).getByRole('button', { name: 'Value' });
  expect(heading.getAttribute('tabindex')).toBeNull();
  button.focus();
  await user.keyboard(' ');
  expect(change).toHaveBeenCalledExactlyOnceWith({ column: 'value', direction: 'asc' });
  expect(heading.getAttribute('aria-sort')).toBe('ascending');
});

it('provides a named keyboard scroll stop even when every column is noninteractive', async () => {
  render(<DataTable {...props} title="Warehouse" ariaLabel="Inventory" columns={[{ key: 'value', label: 'Value', sortable: false }]} />);
  const viewport = screen.getByRole('region', { name: 'Inventory table viewport' });
  expect(viewport.tabIndex).toBe(0);
  expect(screen.getByRole('table', { name: 'Inventory' })).not.toBeNull();
  await userEvent.setup().tab();
  expect(document.activeElement).toBe(viewport);
  expect(screen.queryByRole('button')).toBeNull();
});
