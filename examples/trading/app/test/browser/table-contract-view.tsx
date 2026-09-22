// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

import { useEffect } from 'react';
import { DataTable, type ColumnDef } from '@drasi/react/components';

export interface ContractRow { id: string; value: number | string }
export type ContractMode = 'sorting' | 'alignment';

export function ContractTable({ rows, mode = 'sorting', onCommit }: {
  rows: readonly ContractRow[];
  mode?: ContractMode;
  onCommit?: () => void;
}) {
  useEffect(() => { onCommit?.(); });
  const columns: ColumnDef<ContractRow>[] = mode === 'sorting'
    ? [{ key: 'id', label: 'Identity', sortable: false }, { key: 'value', label: 'Value' }]
    : [
      { key: 'left', label: 'Explicit left', align: 'left', format: (_value, row) => row.value },
      { key: 'value', label: 'Inherited' },
      { key: 'right', label: 'Right', align: 'right', format: (_value, row) => row.value },
      { key: 'center', label: 'Center', align: 'center', format: (_value, row) => row.value },
    ];
  return <DataTable
    rows={rows}
    columns={columns}
    rowKey={row => row.id}
    defaultSort={{ column: 'value', direction: 'asc' }}
    title="Table contract"
  />;
}
