// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import { expect, it } from 'vitest';
import { assertRowPulse, type NativeRowAnimationFacts } from '../browser/rowAnimationFacts';

// Native Firefox coexistence control: same row, independent pulse and hover transition.
const pulse: NativeRowAnimationFacts = {
  type: 'CSSAnimation', animationName: 'drasi-row-flash-repeat', transitionProperty: null,
  targetIsRow: true, duration: 500, timingEasing: 'linear',
  keyframeEasings: ['ease-in-out', 'ease-in-out', 'ease-in-out'],
  currentTimeMs: 0, startTimeMs: null, playState: 'running',
};
const transition: NativeRowAnimationFacts = {
  type: 'CSSTransition', animationName: null, transitionProperty: 'background-color',
  targetIsRow: true, duration: 150, timingEasing: 'ease', keyframeEasings: ['linear', 'linear'],
  currentTimeMs: 28.82, startTimeMs: 1790.06, playState: 'running',
};

it.each(['drasi-row-flash', 'drasi-row-flash-repeat'])('accepts exactly one %s pulse with an optional independent transition in either order', animationName => {
  const observed = { ...pulse, animationName };
  for (const objects of [[observed], [transition, observed], [observed, transition]]) {
    expect(() => assertRowPulse(objects)).not.toThrow();
  }
});

it.each([
  ['no pulse', [transition]],
  ['duplicate pulse', [pulse, { ...pulse, animationName: 'drasi-row-flash' }]],
  ['unexpected native type', [pulse, { ...transition, type: 'Animation' }]],
  ['unexpected pulse name', [{ ...pulse, animationName: 'another-animation' }]],
  ['pulse targeting another row', [{ ...pulse, targetIsRow: false }]],
  ['pulse timing changed', [{ ...pulse, duration: 150 }]],
  ['non-numeric pulse duration', [{ ...pulse, duration: '500ms' }]],
  ['pulse effect easing changed', [{ ...pulse, timingEasing: 'ease' }]],
  ['pulse keyframe easing changed', [{ ...pulse, keyframeEasings: ['linear', 'linear', 'linear'] }]],
  ['pulse misidentified as a transition', [{ ...pulse, transitionProperty: 'background-color' }]],
  ['extra transition', [pulse, transition, transition]],
  ['transition targeting another row', [pulse, { ...transition, targetIsRow: false }]],
  ['unexpected transition property', [pulse, { ...transition, transitionProperty: 'opacity' }]],
  ['transition timing changed', [pulse, { ...transition, duration: 500 }]],
  ['transition easing changed', [pulse, { ...transition, timingEasing: 'linear' }]],
  ['transition misidentified as a pulse', [pulse, { ...transition, animationName: 'drasi-row-flash' }]],
] satisfies [string, NativeRowAnimationFacts[]][])('rejects %s instead of treating any animation as a pulse', (_name, objects) => {
  expect(() => assertRowPulse(objects)).toThrow();
});
