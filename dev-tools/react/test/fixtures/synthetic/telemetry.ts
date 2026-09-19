// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

// Synthetic contracts, not recorded Drasi Server payloads.
export const readings = [
  { device: 'boiler-7', metric: 'temperature', value: '21.5' },
  { device: 'boiler-7', metric: 'pressure', value: '4.0' },
];

export const temperatureUpdate = {
  query_id: 'building-readings',
  results: [{ op: 'u', before: readings[0], after: { ...readings[0], value: '22.0' } }],
};

export const pressureDelete = {
  query_id: 'building-readings',
  results: [{ op: 'd', before: readings[1] }],
};
