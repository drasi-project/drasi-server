// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import type { Page } from '@playwright/test';

type ClockSetupPage = {
  url: () => string;
  clock: Pick<Page['clock'], 'install' | 'pauseAt'>;
};

/** Establish the same paused anchor before any application code is loaded. */
export async function installPausedClock(page: ClockSetupPage, anchor: Date): Promise<void> {
  if (page.url() !== 'about:blank' || !Number.isFinite(anchor.getTime())) {
    throw new Error('Paused clock preparation requires a blank page and a valid anchor');
  }
  // Two protocol calls are not atomic. Start before the anchor so elapsed
  // setup time cannot make pauseAt(anchor) attempt to move backwards.
  await page.clock.install({ time: new Date(anchor.getTime() - 60_000) });
  await page.clock.pauseAt(anchor);
}
