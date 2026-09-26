// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import {
  lazy, Suspense, useEffect, useRef, useState,
  type CSSProperties, type ReactNode, type RefObject,
} from 'react';

export interface ModalProps {
  /** Controlled visibility. Closing/unmounting releases this modal's ownership. */
  open: boolean;
  /** Called once for a topmost Escape/outside dismissal; the owner sets open=false. */
  onClose: () => void;
  /** Required nonempty accessible name; rendered as a visually hidden dialog title. */
  title: string;
  /** Optional short description. Omit for complex structured dialog contents. */
  description?: string;
  /** Include a visible, keyboard-operable close/cancel control. No domain-specific chrome is added. */
  children: ReactNode;
  /** Initial focus inside the dialog; defaults to the dialog itself. */
  initialFocusRef?: RefObject<HTMLElement>;
  /** Preferred return target; otherwise the element focused at opening is used. */
  returnFocusRef?: RefObject<HTMLElement>;
  /** Focusable fallback if the return target has disappeared or become unavailable. */
  fallbackFocusRef?: RefObject<HTMLElement>;
  /** Theme source; defaults to the modal's inline position in the React tree. */
  themeRef?: RefObject<HTMLElement>;
  /** Custom content classes instead of the optional default drasi-modal-surface decoration. */
  className?: string;
  style?: CSSProperties;
  /** Custom overlay classes instead of the default centered, dimmed overlay. */
  overlayClassName?: string;
  overlayStyle?: CSSProperties;
  /** Whether Escape requests closing (default true). Never dismisses a lower layer. */
  closeOnEscape?: boolean;
  /** Whether a primary pointer press outside requests closing (default true). */
  closeOnOutsideClick?: boolean;
}

const ClientModal = lazy(() => import('./ModalLayer').then(module => ({ default: module.ModalLayer })));

/**
 * Controlled Radix-backed modal, without a provider or tutorial/data coupling.
 * SSR and initial hydration emit only a hidden theme anchor, without a portal
 * or browser reads. The client layer
 * loads on first mount (mounting closed warms it up); only a ready, open layer
 * acquires focus/scroll ownership. Closing/unmounting while loading cannot open
 * a late overlay. Loading errors propagate to the owner's React error boundary.
 * Import styles.css explicitly.
 */
export function Modal(props: ModalProps): ReactNode {
  if (typeof props.title !== 'string' || props.title.trim().length === 0) {
    throw new TypeError('Modal title must be a nonempty accessible name.');
  }
  const [mounted, setMounted] = useState(false);
  const wasOpen = useRef(false);
  const trigger = useRef<HTMLElement | null>(null);
  const anchor = useRef<HTMLSpanElement>(null);
  useEffect(() => { setMounted(true); }, []);
  useEffect(() => {
    if (props.open && !wasOpen.current && document.activeElement instanceof HTMLElement) {
      trigger.current = document.activeElement;
    }
    wasOpen.current = props.open;
  }, [props.open]);
  return <>
    <span hidden ref={anchor} data-drasi-modal-anchor="" />
    {mounted && (
      <Suspense fallback={null}>
        <ClientModal {...props} themeRef={props.themeRef ?? anchor} returnFocusRef={props.returnFocusRef ?? trigger} />
      </Suspense>
    )}
  </>;
}
