// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { useSyncExternalStore } from 'react';

const query = '(prefers-reduced-motion: reduce)';
const media = () => globalThis.matchMedia?.(query);
const snapshot = () => media()?.matches ?? false;
const serverSnapshot = () => false;
function subscribe(change: () => void): () => void {
  const preference = media();
  preference?.addEventListener('change', change);
  return () => preference?.removeEventListener('change', change);
}

/**
 * Live OS/browser motion preference, without UI or CSS imports. SSR and hosts
 * without matchMedia report false; shipped CSS also honors the media query.
 */
export function useReducedMotion(): boolean {
  return useSyncExternalStore(subscribe, snapshot, serverSnapshot);
}
