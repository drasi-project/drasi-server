// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { StrictMode } from 'react';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import * as components from '../src/components';
import { DrasiProvider } from '../src/react';
import { fakeEventSourceFactory } from './FakeEventSource';
import { ReadServer, json, refs } from './server';

async function liveTable(onSortChange = vi.fn()) {
  const server = new ReadServer();
  server.snapshot = async () => json([{ id: 'rack-b', value: 2 }, { id: 'rack-a', value: 1 }]);
  const stream = fakeEventSourceFactory();
  render(
    <StrictMode>
      <DrasiProvider {...refs} fetch={server.fetch} eventSourceFactory={stream.create}>
        <components.QueryTable
          queryId="stocks"
          queryOptions={{ getKey: row => String(row.id), transform: row => row }}
          rowKey={row => String(row.id)}
          columns={[{ key: 'id', label: 'Rack' }, { key: 'value', label: 'Units' }]}
          onSortChange={onSortChange}
        />
      </DrasiProvider>
    </StrictMode>,
  );
  await waitFor(() => expect(stream.instances).toHaveLength(1));
  act(() => stream.instances[0].open());
  await screen.findByRole('table');
  return { server, onSortChange };
}

it('exports provider-free presentation independently of the live table', () => {
  expect(components).toHaveProperty('DataTable');
});

it('performs only required resource validation reads, never an eager inspector read', async () => {
  const { server } = await liveTable();
  const reads = server.fetch.mock.calls.filter(([input]) =>
    new URL(String(input)).pathname.endsWith('/queries/stocks'),
  );
  // StrictMode cancels one initialization read; the live initialization and
  // subscription each validate the resource. Neither is a tutorial request.
  expect(reads.filter(([, init]) => !init?.signal?.aborted)).toHaveLength(2);
});

it('notifies once per sorting action under StrictMode, not inside an updater', async () => {
  const { onSortChange } = await liveTable();
  const user = userEvent.setup();
  await user.click(screen.getByRole('button', { name: 'Units' }));
  await user.keyboard('{Enter} ');
  expect(onSortChange.mock.calls).toEqual([
    [{ column: 'value', direction: 'asc' }],
    [{ column: 'value', direction: 'desc' }],
    [{ column: 'value', direction: 'asc' }],
  ]);
});
