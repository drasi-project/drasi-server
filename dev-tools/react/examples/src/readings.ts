// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import type { DrasiClientOptions, RowKey } from '@drasi/react/client';
import type { UseDrasiQueryOptions } from '@drasi/react/react';

export interface Reading {
  key: string;
  room: 'North' | 'South';
  celsius: number;
}

export const rooms = [
  { queryId: 'north-room', title: 'North room' },
  { queryId: 'south-room', title: 'South room' },
] as const;

export const probeKey: RowKey = raw => {
  if (typeof raw.probeId !== 'string' || !raw.probeId.trim()) {
    throw new Error('Expected a nonempty probeId, including on deletes');
  }
  return raw.probeId;
};

export const readingOptions: UseDrasiQueryOptions<Reading> = {
  getKey: probeKey,
  transform: raw => {
    const key = probeKey(raw);
    if ((raw.room !== 'North' && raw.room !== 'South') ||
        typeof raw.temperatureC !== 'number' || !Number.isFinite(raw.temperatureC)) {
      throw new Error('Expected a room and finite temperatureC');
    }
    return { key, room: raw.room, celsius: raw.temperatureC };
  },
};

export function connectionOptions(origin: string): DrasiClientOptions {
  return {
    serverUrl: origin,
    instanceId: 'cold-chain',
    queryIds: rooms.map(room => room.queryId),
    reaction: { id: 'cold-chain-events', endpoint: new URL('/events', origin).href },
  };
}
