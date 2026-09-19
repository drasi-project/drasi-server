// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { StrictMode, useRef, useState } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { Modal, type ModalProps } from '../src/components';

afterEach(() => {
  document.body.removeAttribute('style');
  document.body.removeAttribute('tabindex');
});

function Example({ initial = false, ...props }: Partial<ModalProps> & { initial?: boolean }) {
  const [open, setOpen] = useState(initial);
  return <>
    <button onClick={() => setOpen(true)}>Open details</button>
    <button>Background</button>
    <Modal open={open} title="Inventory details" description="Review current inventory." onClose={() => setOpen(false)} {...props}>
      <label>Note<input /></label>
      <button onClick={() => setOpen(false)}>Close details</button>
    </Modal>
  </>;
}

it('rejects an empty accessible name instead of opening an unnamed dialog', () => {
  vi.spyOn(console, 'error').mockImplementation(() => {});
  expect(() => render(<Example title="   " />)).toThrow('Modal title must be a nonempty accessible name.');
});

it('names/describes the modal, focuses it, contains both Tab directions and restores the trigger', async () => {
  const user = userEvent.setup();
  render(<StrictMode><Example /></StrictMode>);
  const trigger = screen.getByRole('button', { name: 'Open details' });
  await user.click(trigger);
  const dialog = await screen.findByRole('dialog', { name: 'Inventory details' });
  expect(dialog.getAttribute('aria-modal')).toBe('true');
  expect(document.getElementById(dialog.getAttribute('aria-describedby') ?? '')?.textContent).toBe('Review current inventory.');
  expect(document.activeElement).toBe(dialog);
  expect(screen.queryByRole('button', { name: 'Background' })).toBeNull();
  await user.tab();
  expect(document.activeElement).toBe(screen.getByRole('textbox', { name: 'Note' }));
  await user.tab();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Close details' }));
  await user.tab();
  expect(document.activeElement).toBe(screen.getByRole('textbox'));
  await user.tab({ shift: true });
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Close details' }));
  await user.keyboard('{Escape}');
  await waitFor(() => expect(document.activeElement).toBe(trigger));
  expect(document.body.hasAttribute('data-scroll-locked')).toBe(false);
});

it('allows explicit initial/return focus and omits a description for structured content', async () => {
  const user = userEvent.setup();
  function Explicit() {
    const [open, setOpen] = useState(false);
    const input = useRef<HTMLInputElement>(null);
    const target = useRef<HTMLButtonElement>(null);
    return <>
      <button onClick={() => setOpen(true)}>Open</button><button ref={target}>Return here</button>
      <Modal open={open} title="Details" onClose={() => setOpen(false)} initialFocusRef={input} returnFocusRef={target}>
        <input ref={input} aria-label="Initial" /><button onClick={() => setOpen(false)}>Done</button>
      </Modal>
    </>;
  }
  render(<Explicit />);
  await user.click(screen.getByRole('button', { name: 'Open' }));
  await screen.findByRole('dialog', { name: 'Details' });
  expect(document.activeElement).toBe(screen.getByRole('textbox', { name: 'Initial' }));
  expect(screen.getByRole('dialog').hasAttribute('aria-describedby')).toBe(false);
  await user.click(screen.getByRole('button', { name: 'Done' }));
  await waitFor(() => expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Return here' })));
});

it('never initially focuses an explicit ref outside the active scope', async () => {
  function InvalidInitial() {
    const outside = useRef<HTMLButtonElement>(null);
    return <><button ref={outside}>Outside</button><Example initialFocusRef={outside} /></>;
  }
  render(<InvalidInitial />);
  await userEvent.setup().click(screen.getByRole('button', { name: 'Open details' }));
  const dialog = await screen.findByRole('dialog');
  expect(document.activeElement).toBe(dialog);
});

it.each(['removed', 'disabled', 'hidden', 'inert', 'display', 'visibility'] as const)(
  'uses a safe fallback when the triggering control becomes %s', async invalid => {
    const user = userEvent.setup();
    function Disappearing() {
      const [open, setOpen] = useState(false);
      const [hasTrigger, setHasTrigger] = useState(true);
      const trigger = useRef<HTMLButtonElement>(null);
      const fallback = useRef<HTMLButtonElement>(null);
      return <>
        {hasTrigger && <button ref={trigger} onClick={() => setOpen(true)}>Open</button>}
        <button ref={fallback}>Fallback</button>
        <Modal open={open} title="Details" fallbackFocusRef={fallback} onClose={() => setOpen(false)}>
          <button onClick={() => {
            const element = trigger.current;
            if (!element) throw new Error('Missing trigger');
            if (invalid === 'removed') setHasTrigger(false);
            if (invalid === 'disabled') element.disabled = true;
            if (invalid === 'hidden' || invalid === 'inert') element.setAttribute(invalid, '');
            if (invalid === 'display') element.style.display = 'none';
            if (invalid === 'visibility') element.style.visibility = 'hidden';
            setOpen(false);
          }}>Remove and close</button>
        </Modal>
      </>;
    }
    render(<Disappearing />);
    await user.click(screen.getByRole('button', { name: 'Open' }));
    await user.click(await screen.findByRole('button', { name: 'Remove and close' }));
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Fallback' })));
  },
);

function Layers() {
  const [outer, setOuter] = useState(false);
  const [inner, setInner] = useState(false);
  return <>
    <button onClick={() => setOuter(true)}>Open first</button>
    {outer && <Modal open title="First" onClose={() => setOuter(false)}>
      <button onClick={() => setInner(true)}>Open second</button>
      <button onClick={() => setOuter(false)}>Close first</button>
    </Modal>}
    {inner && <Modal open title="Second" onClose={() => setInner(false)}>
      <button onClick={() => setOuter(false)}>Unmount first</button>
      <button onClick={() => setInner(false)}>Close second</button>
    </Modal>}
  </>;
}

it.each(['outer-first', 'inner-first'] as const)('shares scroll and focus ownership for independent scopes: %s', async order => {
  document.body.style.overflow = 'scroll';
  document.body.style.paddingRight = '7px';
  document.body.style.scrollPadding = '12px';
  const original = document.body.getAttribute('style');
  const user = userEvent.setup();
  render(<StrictMode><Layers /></StrictMode>);
  const first = screen.getByRole('button', { name: 'Open first' });
  await user.click(first);
  const second = await screen.findByRole('button', { name: 'Open second' });
  await user.click(second);
  expect(document.body.getAttribute('data-scroll-locked')).toBe('2');
  expect(screen.queryByRole('dialog', { name: 'First' })).toBeNull();
  if (order === 'outer-first') {
    await user.click(screen.getByRole('button', { name: 'Unmount first' }));
    expect(document.body.getAttribute('data-scroll-locked')).toBe('1');
    expect(screen.getByRole('dialog', { name: 'Second' }).contains(document.activeElement)).toBe(true);
    await user.keyboard('{Escape}');
  } else {
    await user.keyboard('{Escape}');
    await waitFor(() => expect(document.activeElement).toBe(second));
    expect(document.body.getAttribute('data-scroll-locked')).toBe('1');
    expect(screen.getByRole('dialog', { name: 'First' })).not.toBeNull();
    await user.keyboard('{Escape}');
  }
  await waitFor(() => expect(document.body.hasAttribute('data-scroll-locked')).toBe(false));
  expect(document.body.getAttribute('style')).toBe(original);
});

it('limits outside dismissal to the topmost layer and never dismisses from inside', async () => {
  const user = userEvent.setup();
  render(<Layers />);
  await user.click(screen.getByRole('button', { name: 'Open first' }));
  await user.click(await screen.findByRole('button', { name: 'Open second' }));
  const inner = screen.getByRole('dialog', { name: 'Second' });
  await user.click(inner);
  expect(screen.getByRole('dialog', { name: 'Second' })).toBe(inner);
  await user.click(inner.parentElement!);
  expect(screen.queryByRole('dialog', { name: 'Second' })).toBeNull();
  expect(screen.getByRole('dialog', { name: 'First' })).not.toBeNull();
  expect(document.body.getAttribute('data-scroll-locked')).toBe('1');
});

it('respects dismissal opt-outs without installing competing global handlers', async () => {
  const user = userEvent.setup();
  const onClose = vi.fn();
  render(<Example initial closeOnEscape={false} closeOnOutsideClick={false} onClose={onClose} />);
  const dialog = await screen.findByRole('dialog');
  await user.keyboard('{Escape}');
  await user.click(dialog.parentElement!);
  expect(onClose).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(dialog);
});

it('releases all ownership on an open unmount and ignores stale restoration after a rapid reopen', async () => {
  const user = userEvent.setup();
  const view = render(<Example />);
  await user.click(screen.getByRole('button', { name: 'Open details' }));
  await screen.findByRole('dialog');
  const trigger = within(view.container).getByRole('button', { name: 'Open details', hidden: true });
  act(() => {
    fireEvent.click(screen.getByRole('button', { name: 'Close details' }));
    fireEvent.click(trigger);
  });
  await waitFor(() => expect(screen.getByRole('dialog').contains(document.activeElement)).toBe(true));
  view.unmount();
  await waitFor(() => expect(document.body.hasAttribute('data-scroll-locked')).toBe(false));
  await waitFor(() => expect(document.querySelector('[data-radix-focus-guard]')).toBeNull());
  expect(document.body.style.pointerEvents).toBe('');
});

it('falls back to the document when an opening trigger and all scopes unmount', async () => {
  const user = userEvent.setup();
  const view = render(<Example />);
  await user.click(screen.getByRole('button', { name: 'Open details' }));
  await screen.findByRole('dialog');
  document.body.tabIndex = 3;
  view.unmount();
  await waitFor(() => expect(document.activeElement).toBe(document.body));
  expect(document.body.getAttribute('tabindex')).toBe('3');
});

it('reveals wrapped focus inside the owned overlay without moving the background page', async () => {
  const user = userEvent.setup();
  render(<Example initial />);
  const dialog = await screen.findByRole('dialog');
  const overlay = dialog.parentElement!;
  overlay.style.overflow = 'auto';
  overlay.style.border = '0px solid transparent';
  overlay.style.scrollBehavior = 'smooth';
  Object.defineProperties(overlay, {
    clientHeight: { value: 320 }, scrollHeight: { value: 640 }, offsetHeight: { value: 320 },
    clientWidth: { value: 320 }, scrollWidth: { value: 320 }, offsetWidth: { value: 320 },
  });
  vi.spyOn(overlay, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 320, 320));
  const close = within(dialog).getByRole('button', { name: 'Close details' });
  vi.spyOn(close, 'getBoundingClientRect').mockImplementation(() => new DOMRect(20, 600 - overlay.scrollTop, 80, 30));
  const scroll = vi.fn((options: ScrollToOptions) => { overlay.scrollTop = options.top ?? 0; });
  Object.defineProperty(overlay, 'scroll', { value: scroll });
  await user.tab();
  await user.tab({ shift: true });
  expect(document.activeElement).toBe(close);
  expect(scroll).toHaveBeenCalledWith({ top: 310, left: 0, behavior: 'instant' });
  expect(close.getBoundingClientRect().bottom).toBe(320);
  expect(window.scrollY).toBe(0);
  expect(document.documentElement.scrollTop).toBe(0);
});

it('does not apply an outer reveal boundary to a logically nested body portal', async () => {
  const user = userEvent.setup();
  function Nested() {
    const [inner, setInner] = useState(false);
    return <Modal open title="Parent" onClose={() => {}}>
      <button onClick={() => setInner(true)}>Open child</button>
      <Modal open={inner} title="Child" onClose={() => setInner(false)}>
        <button onClick={() => setInner(false)}>Close child</button>
      </Modal>
    </Modal>;
  }
  render(<StrictMode><Nested /></StrictMode>);
  const trigger = await screen.findByRole('button', { name: 'Open child' });
  const parent = screen.getByRole('dialog', { name: 'Parent' });
  await user.click(trigger);
  const child = await screen.findByRole('dialog', { name: 'Child' });
  expect(parent.contains(child)).toBe(false);
  await user.tab();
  expect(child.contains(document.activeElement)).toBe(true);
  await user.keyboard('{Escape}');
  await waitFor(() => expect(document.activeElement).toBe(trigger));
  expect(document.body.getAttribute('data-scroll-locked')).toBe('1');
});
