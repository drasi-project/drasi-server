// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0

type LengthUnit = 'px' | 'rem' | 'em' | 'ch' | 'ex' | 'lh' | 'rlh' |
  'vw' | 'vh' | 'vmin' | 'vmax' | 'svw' | 'svh' | 'lvw' | 'lvh' | 'dvw' | 'dvh' |
  'cm' | 'mm' | 'in' | 'pt' | 'pc' | '%';

/**
 * Nonnegative finite pixels, an explicit CSS length/percentage, auto, or a
 * custom-property reference (with an optional literal length fallback).
 * Percentages need a sized parent. Use a CSS variable for calc()/clamp() rules.
 * Class names such as h-[400px] are not sizes.
 */
export type TableHeight = number | `${number}${LengthUnit}` | '0' | 'auto' | `var(--${string})`;

const length = /^(?:0|(?:\d+(?:\.\d+)?|\.\d+)(?:px|rem|em|ch|ex|lh|rlh|vw|vh|vmin|vmax|svw|svh|lvw|lvh|dvw|dvh|cm|mm|in|pt|pc|%))$/;
const variable = /^var\((--[a-zA-Z_][a-zA-Z0-9_-]*)(?:,\s*([^()]+))?\)$/;

export function tableHeight(value: TableHeight): string {
  if (typeof value === 'number') {
    if (Number.isFinite(value) && value >= 0) return `${value}px`;
  } else {
    const token = variable.exec(value);
    if (value === 'auto' || length.test(value) ||
        (token && (token[2] === undefined || token[2] === 'auto' || length.test(token[2])))) return value;
  }
  throw new TypeError('Table height must be finite nonnegative pixels, a CSS length, auto, or var(--token[, length]); class names are not sizes.');
}
