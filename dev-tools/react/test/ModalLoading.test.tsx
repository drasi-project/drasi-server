// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { StrictMode, useState } from 'react';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import { Modal } from '../src/components/Modal';

const loading = vi.hoisted(() => {
  let release!: () => void;
  const pending = new Promise<void>(resolve => { release = resolve; });
  return { pending, loaded: false, release: () => release() };
});

// Delay the real primitive module, not its focus or ownership implementation.
vi.mock('../src/components/ModalLayer', async importOriginal => {
  await loading.pending;
  const module = await importOriginal<typeof import('../src/components/ModalLayer')>();
  loading.loaded = true;
  return module;
});

it('does not acquire a late overlay after close or unmount during the initial client load', async () => {
  function LoadingOwners() {
    const [first, setFirst] = useState(true);
    const [second, setSecond] = useState(true);
    const [ready, setReady] = useState(false);
    return <>
      <button onClick={() => setFirst(false)}>Cancel pending opening</button>
      <button onClick={() => setSecond(false)}>Unmount pending owner</button>
      <button onClick={() => setReady(true)}>Open ready modal</button>
      <Modal open={first} title="First pending" onClose={() => setFirst(false)}>First</Modal>
      {second && <Modal open title="Second pending" onClose={() => setSecond(false)}>Second</Modal>}
      {ready && <Modal open title="Ready" onClose={() => setReady(false)}>
        <button onClick={() => setReady(false)}>Close ready</button>
      </Modal>}
    </>;
  }
  const user = userEvent.setup();
  render(<StrictMode><LoadingOwners /></StrictMode>);
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(document.body.hasAttribute('data-scroll-locked')).toBe(false);
  await user.click(screen.getByRole('button', { name: 'Cancel pending opening' }));
  await user.click(screen.getByRole('button', { name: 'Unmount pending owner' }));
  await act(async () => { loading.release(); await loading.pending; });
  await waitFor(() => expect(loading.loaded).toBe(true));
  await waitFor(() => expect(document.querySelectorAll('[data-drasi-modal-anchor]')).toHaveLength(1));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(document.body.hasAttribute('data-scroll-locked')).toBe(false);
  const trigger = screen.getByRole('button', { name: 'Open ready modal' });
  await user.click(trigger);
  const modal = await screen.findByRole('dialog', { name: 'Ready' });
  expect(modal.contains(document.activeElement)).toBe(true);
  await user.keyboard('{Escape}');
  await waitFor(() => expect(document.activeElement).toBe(trigger));
  expect(document.body.hasAttribute('data-scroll-locked')).toBe(false);
});
