// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { isAbsolute, relative } from 'node:path';

export function moduleIdentity(raw, root) {
  // Rollup prefixes virtual CommonJS wrappers with NUL, even for absolute file IDs.
  const clean = raw.replace(/^\0+/, '');
  const query = clean.indexOf('?');
  const file = query < 0 ? clean : clean.slice(0, query);
  const suffix = query < 0 ? '' : clean.slice(query);
  const packageFile = /(?:node_modules\/@drasi\/react|dev-tools\/react)\/(dist\/.+)/.exec(file);
  const name = packageFile ? `@drasi/react/${packageFile[1]}` :
    isAbsolute(file) ? relative(root, file).replaceAll('\\', '/') : `virtual:${file}`;
  return { id: name + suffix, sourceMap: packageFile ? `${file}.map` : null };
}

export function assertExampleModule(id) {
  const file = id.split('?')[0];
  const helper = [
    'virtual:commonjsHelpers.js',
    'virtual:vite/modulepreload-polyfill.js',
    'virtual:vite/preload-helper.js',
  ].includes(file);
  const owned = /^(?:src\/|node_modules\/|@drasi\/react\/dist\/)/.test(file);
  assert((helper || owned) && !file.split('/').includes('..') && !file.includes('\0') &&
    !/@drasi\/react\/src(?:\/|$)/.test(file),
  `Example imports outside its source/installed artifacts: ${id}`);
}
