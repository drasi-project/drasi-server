// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToString } from 'react-dom/server';
import { ContractTable } from './table-contract-view';

const locale = new Intl.Collator().resolvedOptions().locale;
assert.equal(locale, 'en-US', 'SSR must run with a real English default locale');
const rows = [{ id: 'a', value: 'A' }, { id: 'accent', value: '\u00c4' }, { id: 'z', value: 'Z' }];
const diagnostics: unknown[][] = [];
console.error = (...args: unknown[]) => { diagnostics.push(args); };
console.warn = (...args: unknown[]) => { diagnostics.push(args); };
const html = renderToString(createElement(ContractTable, { rows }));
assert.deepEqual(diagnostics, [], 'SSR must not warn or error');
process.stdout.write(JSON.stringify({
  locale,
  ambientOrder: rows.map(row => row.value).sort((a, b) => a.localeCompare(b)),
  html,
  rows,
}));
