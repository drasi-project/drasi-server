// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import type { ColumnDef } from '@drasi/react/components';
import type { Reading } from './readings';

export const columns: readonly ColumnDef<Reading>[] = [
  { key: 'key', label: 'Probe' },
  { key: 'room', label: 'Room' },
  { key: 'celsius', label: 'Temperature (C)', align: 'right', format: (_value, row) => row.celsius.toFixed(1) },
];
export const rowKey = (row: Reading) => row.key;
