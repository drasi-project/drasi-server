// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { useLayoutEffect, useState, type CSSProperties, type RefObject } from 'react';
const themeTokens = [
  '--drasi-color-surface', '--drasi-color-dialog', '--drasi-color-code',
  '--drasi-color-border', '--drasi-color-primary', '--drasi-color-success',
  '--drasi-color-danger', '--drasi-color-text', '--drasi-color-muted',
  '--drasi-color-subtle', '--drasi-color-row-border', '--drasi-color-row-hover',
  '--drasi-color-action-hover', '--drasi-color-focus', '--drasi-table-height',
  '--drasi-color-overlay', '--drasi-radius', '--drasi-line-height',
];

/** Copy resolved tokens, not var() expressions whose dependencies stay behind. */
export function usePortalTheme(open: boolean, source: RefObject<HTMLElement>): CSSProperties {
  const [theme, setTheme] = useState<CSSProperties>({});
  useLayoutEffect(() => {
    const element = source.current;
    const view = element?.ownerDocument.defaultView;
    if (!open || !element || !view) return;
    const refresh = () => {
      const computed = view.getComputedStyle(element);
      const tokens = new Set(themeTokens);
      for (let i = 0; i < computed.length; i++) {
        const property = computed.item(i);
        if (property.startsWith('--drasi-')) tokens.add(property);
      }
      const properties: Record<string, string> = {};
      for (const token of tokens) properties[token] = computed.getPropertyValue(token).trim() || 'initial';
      const next: CSSProperties = {
        ...properties,
        fontFamily: computed.fontFamily,
        fontSize: computed.fontSize,
        fontWeight: computed.fontWeight,
        lineHeight: computed.lineHeight,
        direction: computed.direction === 'rtl' ? 'rtl' : 'ltr',
      };
      setTheme(previous => JSON.stringify(previous) === JSON.stringify(next) ? previous : next);
    };
    refresh();
    const observer = new MutationObserver(refresh);
    for (let ancestor: HTMLElement | null = element; ancestor; ancestor = ancestor.parentElement) {
      observer.observe(ancestor, { attributes: true });
    }
    view.addEventListener('resize', refresh);
    const scheme = view.matchMedia?.('(prefers-color-scheme: dark)');
    scheme?.addEventListener('change', refresh);
    return () => {
      observer.disconnect();
      view.removeEventListener('resize', refresh);
      scheme?.removeEventListener('change', refresh);
    };
  }, [open, source]);
  return theme;
}
