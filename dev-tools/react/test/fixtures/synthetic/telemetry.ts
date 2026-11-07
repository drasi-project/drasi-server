// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

// Synthetic contracts, not recorded Drasi Server payloads.
export const readingIds = {
  current: '9f7c254f-064f-4317-848d-bc45dde18a12',
  archive: '2a67e049-d0fb-4a7e-adbb-8f70c230db67',
  events: '895c325b-a75d-4686-a3c5-d525670517b5',
};

export const readings = [
  { device: 'boiler-7', metric: 'temperature', value: '21.5' },
  { device: 'boiler-7', metric: 'pressure', value: '4.0' },
];

export const temperatureUpdate = {
  queryId: readingIds.current,
  timestamp: 1,
  results: [{ type: 'UPDATE', before: readings[0], after: { ...readings[0], value: '22.0' },
    data: { ...readings[0], value: '22.0' } }],
};

export const pressureDelete = {
  queryId: readingIds.current,
  timestamp: 2,
  results: [{ type: 'DELETE', data: { device: readings[1].device, metric: readings[1].metric } }],
};
