// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import type { ReactNode } from 'react';

export function Shell({ title, children }: { title: string; children: ReactNode }) {
  return <main style={{ maxWidth: '64rem', margin: '2rem auto', padding: '0 1rem', fontFamily: 'system-ui, sans-serif' }}>
    <nav aria-label="Examples">
      <a href="/">Live table</a>{' | '}
      <a href="/hooks.html">Hooks only</a>{' | '}
      <a href="/showcase.html">Simulated states</a>
    </nav>
    <h1>{title}</h1>
    {children}
  </main>;
}
