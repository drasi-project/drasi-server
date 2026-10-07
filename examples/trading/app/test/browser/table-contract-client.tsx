// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { createRoot, hydrateRoot, type Root } from 'react-dom/client';
import { ContractTable, type ContractMode, type ContractRow } from './table-contract-view';

declare global {
  interface Window {
    tableContract: {
      commits: number;
      errors: string[];
      render: (rows: ContractRow[], mode?: ContractMode) => void;
      hydrate: (rows: ContractRow[]) => void;
    };
  }
}

let root: Root | undefined;
const container = document.getElementById('table-contract');
if (!container) throw new Error('Missing test table container');
const onCommit = () => { window.tableContract.commits++; };
window.tableContract = {
  commits: 0,
  errors: [],
  render(rows, mode) {
    root ??= createRoot(container);
    root.render(<ContractTable rows={rows} mode={mode} onCommit={onCommit} />);
  },
  hydrate(rows) {
    if (root) throw new Error('Test root already mounted');
    root = hydrateRoot(container, <ContractTable rows={rows} onCommit={onCommit} />, {
      onRecoverableError(error) {
        window.tableContract.errors.push(error instanceof Error ? error.message : String(error));
      },
    });
  },
};
