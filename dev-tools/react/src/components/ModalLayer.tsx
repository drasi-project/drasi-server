// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { useRef, type ReactNode, type RefObject } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import clsx from 'clsx';
import scrollIntoView from 'scroll-into-view-if-needed';
import { usePortalTheme } from './usePortalTheme';
import type { ModalProps } from './Modal';

function focusAvailable(element: HTMLElement | null | undefined): boolean {
  if (!element?.isConnected || element.matches(':disabled') ||
      element.closest('[hidden], [inert], [aria-hidden="true"]')) return false;
  const view = element.ownerDocument.defaultView;
  for (let ancestor: HTMLElement | null = element; ancestor; ancestor = ancestor.parentElement) {
    const style = view?.getComputedStyle(ancestor);
    if (style?.display === 'none' || style?.visibility === 'hidden') return false;
  }
  element.focus({ preventScroll: true });
  return element.ownerDocument.activeElement === element;
}

/** Loaded only after client mount, so primitive DOM probes never run in SSR. */
export function ModalLayer({
  open, onClose, title, description, children,
  initialFocusRef, returnFocusRef, fallbackFocusRef, themeRef,
  className, style, overlayClassName, overlayStyle,
  closeOnEscape = true, closeOnOutsideClick = true,
}: ModalProps & { themeRef: RefObject<HTMLElement> }): ReactNode {
  const content = useRef<HTMLDivElement | null>(null);
  const overlay = useRef<HTMLDivElement>(null);
  const openedFrom = useRef<HTMLElement | null>(null);
  const theme = usePortalTheme(open, themeRef);

  return (
    <Dialog.Root open={open} onOpenChange={next => { if (!next) onClose(); }}>
      <Dialog.Portal>
        <Dialog.Overlay
          ref={overlay}
          className={clsx('drasi-modal-layer', overlayClassName ?? 'drasi-modal-overlay')}
          style={{ ...theme, ...overlayStyle }}
        >
          <Dialog.Content
            ref={element => { if (element) content.current = element; }}
            className={clsx('drasi-modal-content', className ?? 'drasi-modal-surface')}
            style={style}
            data-drasi-modal-content=""
            aria-modal="true"
            {...(description ? {} : { 'aria-describedby': undefined })}
            onFocusCapture={event => {
              const target = event.target;
              // Radix's loop uses preventScroll. Reveal wrapped focus only in
              // this overlay, never in a background page or a nested portal.
              if (overlay.current && target !== event.currentTarget && event.currentTarget.contains(target)) {
                scrollIntoView(target, {
                  boundary: overlay.current,
                  block: 'nearest',
                  inline: 'nearest',
                  scrollMode: 'if-needed',
                  behavior: 'instant',
                });
              }
            }}
            onOpenAutoFocus={event => {
              event.preventDefault();
              const element = content.current;
              if (!element) return;
              const active = element.ownerDocument.activeElement;
              if (active instanceof HTMLElement && !element.contains(active)) openedFrom.current = active;
              if (!element.contains(initialFocusRef?.current ?? null) ||
                  !focusAvailable(initialFocusRef?.current)) element.focus({ preventScroll: true });
            }}
            onCloseAutoFocus={event => {
              event.preventDefault();
              // StrictMode replay or a rapid reopen must not steal the new scope's focus.
              if (content.current?.isConnected) return;
              const document = content.current?.ownerDocument;
              if (!document) return;
              const layers = document.querySelectorAll<HTMLElement>('[data-drasi-modal-content]');
              const top = layers.item(layers.length - 1);
              const candidates = [returnFocusRef?.current, openedFrom.current, fallbackFocusRef?.current];
              for (const candidate of candidates) {
                if ((!top || (candidate && top.contains(candidate))) && focusAvailable(candidate)) return;
              }
              if (top) {
                if (!top.contains(document.activeElement)) focusAvailable(top);
                return;
              }
              const previousTabIndex = document.body.getAttribute('tabindex');
              document.body.tabIndex = -1;
              document.body.focus({ preventScroll: true });
              if (previousTabIndex === null) document.body.removeAttribute('tabindex');
              else document.body.setAttribute('tabindex', previousTabIndex);
            }}
            onEscapeKeyDown={event => { if (!closeOnEscape) event.preventDefault(); }}
            onPointerDownOutside={event => {
              // Keep focus in the scope while a controlled close is pending.
              event.detail.originalEvent.preventDefault();
              if (!closeOnOutsideClick) event.preventDefault();
            }}
          >
            <Dialog.Title asChild><span className="drasi-visually-hidden">{title}</span></Dialog.Title>
            {description && <Dialog.Description className="drasi-visually-hidden">{description}</Dialog.Description>}
            {children}
          </Dialog.Content>
        </Dialog.Overlay>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
