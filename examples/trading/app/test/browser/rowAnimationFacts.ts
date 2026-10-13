// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

import assert from 'node:assert/strict';

export interface NativeRowAnimationFacts {
  type: string;
  animationName: string | null;
  transitionProperty: string | null;
  targetIsRow: boolean;
  duration: number | string | undefined;
  timingEasing: string | undefined;
  keyframeEasings: (string | undefined)[];
  currentTimeMs: number | null;
  startTimeMs: number | null;
  playState: string;
}

export function assertRowPulse(objects: readonly NativeRowAnimationFacts[]): void {
  const pulses = objects.filter(object => object.type === 'CSSAnimation');
  const transitions = objects.filter(object => object.type === 'CSSTransition');
  assert.equal(objects.length, pulses.length + transitions.length, 'Unexpected native animation type');
  assert.equal(pulses.length, 1, 'Expected exactly one native row pulse');
  const pulse = pulses[0];
  assert(['drasi-row-flash', 'drasi-row-flash-repeat'].includes(pulse.animationName ?? ''), 'Unexpected row pulse name');
  assert.equal(pulse.targetIsRow, true, 'Row pulse must target the original row');
  assert.equal(pulse.transitionProperty, null, 'Row pulse must not be a transition');
  assert.equal(pulse.duration, 500, 'Row pulse duration changed');
  assert.equal(pulse.timingEasing, 'linear', 'Row pulse effect easing changed');
  assert.deepEqual(pulse.keyframeEasings, ['ease-in-out', 'ease-in-out', 'ease-in-out'], 'Row pulse keyframe easing changed');
  assert(transitions.length <= 1, 'Unexpected extra row transition');
  for (const transition of transitions) {
    assert.equal(transition.animationName, null, 'Row transition must not be a pulse');
    assert.equal(transition.transitionProperty, 'background-color', 'Unexpected row transition property');
    assert.equal(transition.targetIsRow, true, 'Row transition must target the original row');
    assert.equal(transition.duration, 150, 'Row transition duration changed');
    assert.equal(transition.timingEasing, 'ease', 'Row transition easing changed');
  }
}
