// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it } from 'vitest';
import { panel, renderTrading } from './renderTrading';

it('dismisses a nested inspector with Escape without also collapsing its expanded table', async () => {
  await renderTrading();
  const user = userEvent.setup();
  await user.click(within(panel('Watchlist')).getByRole('button', { name: 'Expand table' }));
  await waitFor(() => expect(document.querySelector('.drasi-query-table--expanded')).not.toBeNull());
  const expanded = document.querySelector<HTMLElement>('.drasi-query-table--expanded');
  if (!expanded) throw new Error('Expected expanded table');
  expect(within(expanded).getByRole('button', { name: 'Collapse table' })).not.toBeNull();
  await user.click(within(expanded).getByRole('button', { name: 'View code' }));
  await screen.findByRole('tab', { name: 'Query Definition' });
  await user.keyboard('{Escape}');
  await waitFor(() => expect(screen.queryByRole('tab')).toBeNull());
  await new Promise(resolve => setTimeout(resolve, 400));
  expect(document.querySelector('.drasi-query-table--expanded')).not.toBeNull();
  expect(within(expanded).getByRole('button', { name: 'View code' })).toBe(document.activeElement);
});

it('focuses an opened Trading dialog and restores its triggering button on close', async () => {
  await renderTrading();
  const user = userEvent.setup();
  const trigger = within(panel('Portfolio')).getByRole('button', { name: 'Add position' });
  await user.click(trigger);
  const dialog = await screen.findByRole('dialog', { name: 'Add Position' });
  expect(dialog.contains(document.activeElement)).toBe(true);
  fireEvent.keyDown(document, { key: 'Escape' });
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(trigger));
});
