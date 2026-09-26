// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { useState } from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { CodeViewerDialog } from '../../src/components/CodeViewerDialog';
import { OrderDialog } from '../../src/components/OrderDialog';
import { PlaceholderTable } from '../../src/components/PlaceholderTable';
import { PositionDialog } from '../../src/components/PositionDialog';
import { BaseDialog, DialogButton } from '../../src/components/shared/BaseDialog';
import { SelectDialog } from '../../src/components/shared/SelectDialog';
import type { Stock } from '../../src/services/TradingApi';

const viewerProps = {
  isOpen: true,
  onClose: () => {},
  title: 'Watchlist code',
  reactCode: '<TradingQueryTable />',
  cypherQuery: 'MATCH (s:Stock) RETURN s',
};

const stocks: Stock[] = [
  { symbol: 'AAPL', name: 'Apple', sector: 'Technology', industry: 'Hardware' },
  { symbol: 'MSFT', name: 'Microsoft', sector: 'Technology', industry: 'Software' },
];

function deferred() {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

function descriptionOf(element: HTMLElement): string {
  return (element.getAttribute('aria-describedby') ?? '').split(/\s+/)
    .map(id => document.getElementById(id)?.textContent ?? '').join(' ');
}

function topLayer(): HTMLElement {
  const layers = document.querySelectorAll<HTMLElement>('.drasi-modal-layer');
  const layer = layers[layers.length - 1];
  if (!layer) throw new Error('Expected a shared modal layer');
  return layer;
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe('Trading code viewer accessibility', () => {
  it('has automatically activating, roving tabs and named keyboard-scrollable panels', async () => {
    const user = userEvent.setup();
    render(<CodeViewerDialog {...viewerProps} drasiUiUrl="http://localhost:8280/ui?instance=trading-server" />);
    const dialog = await screen.findByRole('dialog', { name: viewerProps.title });
    const tabs = within(dialog).getByRole('tablist', { name: 'Code examples' });
    const query = within(tabs).getByRole('tab', { name: 'Query Definition' });
    const react = within(tabs).getByRole('tab', { name: 'React Code' });
    const queryPanel = within(dialog).getByRole('tabpanel', { name: 'Query Definition' });
    await waitFor(() => expect(tabs.tabIndex).toBe(0));
    expect(react.tabIndex).toBe(-1);
    expect(query.getAttribute('aria-selected')).toBe('true');
    expect(query.getAttribute('aria-controls')).toBe(queryPanel.id);
    expect(queryPanel.getAttribute('aria-labelledby')).toBe(query.id);
    expect(queryPanel.tabIndex).toBe(0);
    expect(tabs.contains(within(dialog).getByRole('button', { name: 'Copy' }))).toBe(false);
    expect(dialog.contains(document.activeElement)).toBe(true);

    await user.tab();
    await user.tab();
    await user.tab();
    await waitFor(() => expect(document.activeElement).toBe(query));
    expect(query.tabIndex).toBe(0);
    for (const [key, expected] of [
      ['{ArrowRight}', react],
      ['{ArrowLeft}', query],
      ['{End}', react],
      ['{Home}', query],
      ['{ArrowLeft}', react],
      ['{ArrowRight}', query],
    ] as const) {
      await user.keyboard(key);
      await waitFor(() => expect(document.activeElement).toBe(expected));
      expect(expected.getAttribute('aria-selected')).toBe('true');
      expect(expected.tabIndex).toBe(0);
      const panel = within(dialog).getByRole('tabpanel');
      expect(expected.getAttribute('aria-controls')).toBe(panel.id);
      expect(panel.getAttribute('aria-labelledby')).toBe(expected.id);
      expect(panel.tabIndex).toBe(0);
    }

    const link = within(dialog).getByRole('link', { name: 'Open in Drasi UI' });
    expect(link.getAttribute('target')).toBe('_blank');
    expect(link.getAttribute('rel')).toBe('noopener noreferrer');
    expect(link.getAttribute('href')).toBe('http://localhost:8280/ui?instance=trading-server');
    for (const icon of dialog.querySelectorAll('svg')) {
      expect(icon.getAttribute('aria-hidden')).toBe('true');
      expect(icon.getAttribute('focusable')).toBe('false');
    }
  });

  it('keeps asynchronously updated text live for both panels and clipboard, and reopens on Query Definition', async () => {
    const user = userEvent.setup();
    const view = render(<CodeViewerDialog {...viewerProps} cypherQuery="Loading query definition..." />);
    view.rerender(<CodeViewerDialog {...viewerProps} cypherQuery="MATCH (s) RETURN s.symbol" reactCode="consumer v2" />);
    expect((await screen.findByRole('tabpanel')).textContent).toBe('MATCH (s) RETURN s.symbol');
    await user.click(screen.getByRole('button', { name: 'Copy' }));
    expect(await navigator.clipboard.readText()).toBe('MATCH (s) RETURN s.symbol');
    await user.click(screen.getByRole('tab', { name: 'React Code' }));
    expect(screen.getByRole('tabpanel').textContent).toBe('consumer v2');
    await user.click(screen.getByRole('button', { name: 'Copied!' }));
    expect(await navigator.clipboard.readText()).toBe('consumer v2');
    view.rerender(<CodeViewerDialog {...viewerProps} isOpen={false} />);
    view.rerender(<CodeViewerDialog {...viewerProps} />);
    expect(screen.getByRole('tab', { name: 'Query Definition' }).getAttribute('aria-selected')).toBe('true');
    expect(screen.getByRole('button', { name: 'Copy' })).not.toBeNull();
  });

  it('does not leave success-shaped feedback after a clipboard failure', async () => {
    const user = userEvent.setup();
    const denied = new Error('Clipboard permission denied');
    const log = vi.spyOn(console, 'error').mockImplementation(() => {});
    const write = vi.spyOn(navigator.clipboard, 'writeText')
      .mockResolvedValueOnce(undefined).mockRejectedValueOnce(denied);
    render(<CodeViewerDialog {...viewerProps} />);
    await user.click(await screen.findByRole('button', { name: 'Copy' }));
    await user.click(screen.getByRole('button', { name: 'Copied!' }));
    expect(write).toHaveBeenCalledTimes(2);
    expect(log).toHaveBeenCalledWith('Failed to copy:', denied);
    expect(screen.queryByRole('button', { name: 'Copied!' })).toBeNull();
    expect(screen.getByRole('status').textContent).toContain('Unable to copy code.');
  });

  it('ignores an older clipboard result once a newer attempt has completed', async () => {
    const user = userEvent.setup();
    const old = deferred();
    const current = deferred();
    vi.spyOn(navigator.clipboard, 'writeText')
      .mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
    const log = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(<CodeViewerDialog {...viewerProps} />);
    await user.click(await screen.findByRole('button', { name: 'Copy' }));
    await user.click(screen.getByRole('button', { name: 'Copy' }));
    await act(async () => current.resolve());
    expect(screen.getByRole('button', { name: 'Copied!' })).not.toBeNull();
    await act(async () => old.reject(new Error('Earlier request failed')));
    expect(log).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: 'Copied!' })).not.toBeNull();
    expect(screen.getByRole('status').textContent).toBe('Code copied to clipboard.');
  });

  it.each(['close', 'unmount'] as const)('ignores pending clipboard completion after %s', async (action) => {
    userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    const pending = deferred();
    vi.spyOn(navigator.clipboard, 'writeText').mockReturnValue(pending.promise);
    const view = render(<CodeViewerDialog {...viewerProps} />);
    const copy = await screen.findByRole('button', { name: 'Copy' });
    vi.useFakeTimers();
    fireEvent.click(copy);
    if (action === 'close') {
      view.rerender(<CodeViewerDialog {...viewerProps} isOpen={false} />);
      view.rerender(<CodeViewerDialog {...viewerProps} />);
    } else {
      view.unmount();
    }
    act(() => vi.advanceTimersByTime(100));
    const timersBeforeCompletion = vi.getTimerCount();
    await act(async () => pending.resolve());
    expect(vi.getTimerCount()).toBe(timersBeforeCompletion);
    expect(screen.queryByRole('button', { name: 'Copied!' })).toBeNull();
  });

  it.each(['close', 'unmount'] as const)('clears scheduled clipboard feedback on %s', async (action) => {
    userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
    const view = render(<CodeViewerDialog {...viewerProps} />);
    const copy = await screen.findByRole('button', { name: 'Copy' });
    vi.useFakeTimers();
    await act(async () => fireEvent.click(copy));
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    if (action === 'close') view.rerender(<CodeViewerDialog {...viewerProps} isOpen={false} />);
    else view.unmount();
    act(() => vi.advanceTimersByTime(100));
    expect(vi.getTimerCount()).toBe(0);
  });

  it('resets successful clipboard feedback after the original two-second interval', async () => {
    userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
    render(<CodeViewerDialog {...viewerProps} />);
    const copy = await screen.findByRole('button', { name: 'Copy' });
    vi.useFakeTimers();
    await act(async () => fireEvent.click(copy));
    act(() => vi.advanceTimersByTime(1999));
    expect(screen.getByRole('button', { name: 'Copied!' })).not.toBeNull();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.getByRole('button', { name: 'Copy' })).not.toBeNull();
  });
});

describe('Trading form labels and unchanged validation', () => {
  it('labels the stock selector and keeps confirmation actions as non-submit buttons', async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    render(<SelectDialog
      isOpen onClose={() => {}} onConfirm={onConfirm} title="Add to Watchlist"
      selectLabel="Select Stock" options={stocks.map(stock => ({ value: stock.symbol, label: stock.name }))}
    />);
    const select = await screen.findByRole('combobox', { name: 'Select Stock' });
    await user.selectOptions(select, 'MSFT');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    expect(onConfirm).toHaveBeenCalledWith('MSFT');
    for (const button of screen.getAllByRole('button')) expect(button.getAttribute('type')).toBe('button');
  });

  it('associates position labels and errors without changing submitted values', async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PositionDialog
      isOpen mode="add" availableStocks={stocks} isSubmitting={false}
      onSubmit={onSubmit} onCancel={() => {}}
    />);
    expect(await screen.findByRole('combobox', { name: 'Stock' })).not.toBeNull();
    const quantity = screen.getByRole('spinbutton', { name: 'Quantity' });
    const price = screen.getByRole('spinbutton', { name: 'Purchase Price ($)' });
    const date = screen.getByLabelText('Purchase Date');
    fireEvent.change(date, { target: { value: '' } });
    await user.click(screen.getByRole('button', { name: 'Add' }));
    expect(onSubmit).not.toHaveBeenCalled();
    for (const [control, message] of [
      [quantity, 'Quantity is required'],
      [price, 'Purchase price is required'],
      [date, 'Purchase date is required'],
    ] as const) {
      expect(control.getAttribute('aria-invalid')).toBe('true');
      expect(descriptionOf(control)).toBe(message);
    }
    await user.type(quantity, '100');
    await user.type(price, '150.25');
    fireEvent.change(date, { target: { value: '2020-01-01' } });
    for (const control of [quantity, price, date]) {
      expect(control.getAttribute('aria-invalid')).toBe('false');
      expect(control.getAttribute('aria-describedby')).toBeNull();
    }
    await user.click(screen.getByRole('button', { name: 'Add' }));
    expect(onSubmit).toHaveBeenCalledWith({
      id: undefined, symbol: 'AAPL', name: 'Apple', quantity: 100,
      purchasePrice: 150.25, purchaseDate: '2020-01-01',
    });
  });

  it('labels order inputs and exposes selected order type, errors and the existing expiry hint', async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<OrderDialog
      isOpen availableStocks={stocks} isSubmitting={false} onSubmit={onSubmit} onCancel={() => {}}
    />);
    expect(await screen.findByRole('combobox', { name: 'Stock' })).not.toBeNull();
    const types = screen.getByRole('group', { name: 'Order Type' });
    const buy = within(types).getByRole('button', { name: 'Buy' });
    const sell = within(types).getByRole('button', { name: 'Sell' });
    expect(buy.getAttribute('aria-pressed')).toBe('true');
    expect(sell.getAttribute('aria-pressed')).toBe('false');
    await user.click(sell);
    expect(buy.getAttribute('aria-pressed')).toBe('false');
    expect(sell.getAttribute('aria-pressed')).toBe('true');
    expect(descriptionOf(types)).toBe('Sell when price rises to target');

    const price = screen.getByRole('spinbutton', { name: 'Target Price ($)' });
    const quantity = screen.getByRole('spinbutton', { name: 'Quantity' });
    const expires = screen.getByRole('spinbutton', { name: 'Expires In (seconds)' });
    expect(descriptionOf(expires)).toContain('demonstrates drasi.trueLater');
    await user.clear(expires);
    await user.type(expires, '9');
    await user.click(screen.getByRole('button', { name: 'Create Order' }));
    expect(onSubmit).not.toHaveBeenCalled();
    for (const [control, message] of [
      [price, 'Target price is required'],
      [quantity, 'Quantity is required'],
      [expires, 'Must be at least 10 seconds'],
    ] as const) {
      expect(control.getAttribute('aria-invalid')).toBe('true');
      expect(descriptionOf(control)).toContain(message);
    }
    await user.type(price, '125');
    await user.type(quantity, '10');
    await user.clear(expires);
    await user.type(expires, '60');
    expect(expires.getAttribute('aria-invalid')).toBe('false');
    expect(descriptionOf(expires)).not.toContain('Must be at least');
    expect(descriptionOf(expires)).toContain('demonstrates drasi.trueLater');
    await user.click(screen.getByRole('button', { name: 'Create Order' }));
    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({
      symbol: 'AAPL', orderType: 'sell', targetPrice: 125, quantity: 10,
      staleDuration: 30, expireDuration: 60,
    }));
  });
});

describe('Trading dialogs using the shared modal', () => {
  it('preserves the explicit Escape and overlay dismissal opt-outs', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const props = { isOpen: true, onClose, title: 'Confirmation', children: <p>Review this change.</p> };
    const view = render(<BaseDialog {...props} closeOnEscape={false} closeOnOverlayClick={false} />);
    await screen.findByRole('dialog', { name: props.title });
    await user.keyboard('{Escape}');
    await user.click(topLayer());
    expect(onClose).not.toHaveBeenCalled();
    view.rerender(<BaseDialog {...props} closeOnOverlayClick={false} />);
    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalledTimes(1);
    onClose.mockClear();
    view.rerender(<BaseDialog {...props} closeOnEscape={false} />);
    await user.click(topLayer());
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it.each([true, false])('keeps the outer modal active with nested Escape dismissal set to %s', async (closeOnEscape) => {
    const user = userEvent.setup();
    function StackedDialogs() {
      const [outerOpen, setOuterOpen] = useState(false);
      const [innerOpen, setInnerOpen] = useState(false);
      return <>
        <button type="button" onClick={() => setOuterOpen(true)}>Open position</button>
        <BaseDialog isOpen={outerOpen} onClose={() => setOuterOpen(false)} title="Outer position">
          <button type="button" onClick={() => setInnerOpen(true)}>Review position</button>
          <BaseDialog
            isOpen={innerOpen} onClose={() => setInnerOpen(false)} title="Inner confirmation"
            closeOnEscape={closeOnEscape}
            footer={<DialogButton onClick={() => setInnerOpen(false)}>Back</DialogButton>}
          >
            <p>Confirm position details.</p>
          </BaseDialog>
        </BaseDialog>
      </>;
    }
    document.body.style.overflow = 'scroll';
    render(<StackedDialogs />);
    const trigger = screen.getByRole('button', { name: 'Open position' });
    await user.click(trigger);
    const outer = await screen.findByRole('dialog', { name: 'Outer position' });
    expect(outer.contains(document.activeElement)).toBe(true);
    const nestedTrigger = within(outer).getByRole('button', { name: 'Review position' });
    await user.click(nestedTrigger);
    const inner = await screen.findByRole('dialog', { name: 'Inner confirmation' });
    expect(inner.contains(document.activeElement)).toBe(true);
    await user.keyboard('{Escape}');
    if (!closeOnEscape) {
      expect(screen.getByRole('dialog', { name: 'Inner confirmation' })).toBe(inner);
      expect(outer.isConnected).toBe(true);
      await user.click(screen.getByRole('button', { name: 'Back' }));
    }
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Inner confirmation' })).toBeNull());
    expect(screen.getByRole('dialog', { name: 'Outer position' })).toBe(outer);
    expect(document.body.hasAttribute('data-scroll-locked')).toBe(true);
    await waitFor(() => expect(document.activeElement).toBe(nestedTrigger));
    await user.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(trigger));
    expect(document.body.hasAttribute('data-scroll-locked')).toBe(false);
    expect(document.body.style.overflow).toBe('scroll');
  });
});

it('applies placeholder heights as CSS lengths while retaining the default 400px dimension', () => {
  const view = render(<PlaceholderTable title="Planned table" />);
  const card = screen.getByRole('heading', { name: 'Planned table' }).parentElement?.parentElement;
  expect(card?.style.height).toBe('400px');
  expect(card?.className).not.toContain('h-[');
  view.rerender(<PlaceholderTable title="Planned table" height="28rem" />);
  expect(card?.style.height).toBe('28rem');
  view.rerender(<PlaceholderTable title="Planned table" height="var(--planned-table-height)" />);
  expect(card?.style.height).toBe('var(--planned-table-height)');
});
