// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import { Component, StrictMode, type ErrorInfo, type ReactNode } from 'react';
import { act, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { Modal } from '../src/components/Modal';

const loading = vi.hoisted(() => {
  const error = new Error('Modal client chunk unavailable');
  let release!: () => void;
  const pending = new Promise<void>(resolve => { release = resolve; });
  return { error, pending, release: () => release() };
});

vi.mock('../src/components/ModalLayer', async () => {
  await loading.pending;
  // Reject Modal's actual lazy-loader promise, not Vitest's separately wrapped factory promise.
  return { get ModalLayer(): never { throw loading.error; } };
});

it('propagates a rejected client load to the owner boundary without acquiring late modal ownership', async () => {
  const caught = vi.fn<(error: Error, info: ErrorInfo) => void>();
  class OwnerBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
    state: { error: Error | null } = { error: null };
    static getDerivedStateFromError(error: Error) { return { error }; }
    componentDidCatch(error: Error, info: ErrorInfo) { caught(error, info); }
    render() {
      return this.state.error
        ? <p role="alert">Dialog unavailable: {this.state.error.message}</p>
        : this.props.children;
    }
  }
  const diagnostics: unknown[][] = [];
  const consoleError = vi.spyOn(console, 'error').mockImplementation((...args: unknown[]) => {
    diagnostics.push(args);
  });
  const closed = vi.fn();
  const trigger = document.createElement('button');
  trigger.textContent = 'Outside owner';
  document.body.append(trigger);
  trigger.focus();
  const before = {
    overflow: document.body.style.overflow,
    pointerEvents: document.body.style.pointerEvents,
    ariaHidden: trigger.getAttribute('aria-hidden'),
  };
  const rendered = render(<StrictMode><OwnerBoundary>
    <Modal open title="Pending details" onClose={closed}><button>Dialog action</button></Modal>
  </OwnerBoundary></StrictMode>);
  const noOwnership = () => {
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(document.querySelector('.drasi-modal-layer, [data-drasi-modal-content]')).toBeNull();
    expect(document.body.hasAttribute('data-scroll-locked')).toBe(false);
    expect(document.body.style.overflow).toBe(before.overflow);
    expect(document.body.style.pointerEvents).toBe(before.pointerEvents);
    expect(trigger.getAttribute('aria-hidden')).toBe(before.ariaHidden);
    expect(document.activeElement).toBe(trigger);
    expect(closed).not.toHaveBeenCalled();
  };
  try {
    await waitFor(() => expect(document.querySelector('[data-drasi-modal-anchor]')).not.toBeNull());
    noOwnership();
    await act(async () => { loading.release(); await loading.pending; });
    await screen.findByRole('alert');
    expect(screen.getByRole('alert').textContent).toBe('Dialog unavailable: Modal client chunk unavailable');
    expect(caught).toHaveBeenCalled();
    for (const [error, info] of caught.mock.calls) {
      expect(error).toBe(loading.error);
      expect(info.componentStack).toContain('Modal');
    }
    await act(async () => { await Promise.resolve(); });
    noOwnership();
    expect(document.querySelector('[data-drasi-modal-anchor]')).toBeNull();
    rendered.unmount();
    noOwnership();
    expect(diagnostics.length).toBeGreaterThan(0);
    for (const args of diagnostics) {
      const text = args.map(String).join(' ');
      if (text.startsWith('The above error occurred in one of your React components:')) {
        expect(text).toContain('at Lazy');
        expect(text).toContain('at Modal');
        expect(text).toContain('OwnerBoundary');
        expect(text).toContain('React will try to recreate this component tree');
      } else {
        expect(text).toContain(loading.error.message);
      }
    }
    expect(diagnostics.some(args => args.map(String).join(' ').includes('OwnerBoundary'))).toBe(true);
  } finally {
    rendered.unmount();
    trigger.remove();
    consoleError.mockRestore();
  }
});
