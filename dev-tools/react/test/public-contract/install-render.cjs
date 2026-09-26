// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
const assert = require('node:assert/strict');
const { createRequire } = require('node:module');

async function main() {
  const [format, expectation] = process.argv.slice(2);
  assert(['esm', 'cjs'].includes(format));
  assert(['single-react', 'duplicate-react'].includes(expectation));
  const React = require('react');
  const { renderToString } = require('react-dom/server');
  const packageRequire = createRequire(require.resolve('@drasi/react/components'));
  const rendererRequire = createRequire(require.resolve('react-dom/server'));
  assert.equal(require('react/package.json').version, '18.3.1');
  assert.equal(require('react-dom/package.json').version, '18.3.1');
  assert.equal(rendererRequire('react'), React);
  const sameReact = packageRequire('react') === React;
  assert.equal(sameReact, expectation === 'single-react');
  const diagnostics = [];
  const originalError = console.error;
  const originalWarn = console.warn;
  console.error = (...args) => diagnostics.push(args.map(String).join(' '));
  console.warn = (...args) => diagnostics.push(args.map(String).join(' '));
  try {
    const api = format === 'esm' ? await import('@drasi/react/components') : require('@drasi/react/components');
    const render = () => renderToString(React.createElement(api.DataTable, {
      title: 'Fresh installed consumer',
      rows: [{ readingId: 'sensor-independent', value: 18 }],
      rowKey: row => row.readingId,
      columns: [{ key: 'readingId', label: 'Reading' }, { key: 'value', label: 'Value' }],
    }));
    if (expectation === 'duplicate-react') {
      assert.throws(render, /useSyncExternalStore|Invalid hook call/);
      assert(diagnostics.some(message => /Invalid hook call/.test(message)),
        'Unsupported default file-link control did not reproduce its React identity warning');
    } else {
      const html = render();
      assert.match(html, /<table/);
      assert(html.includes('sensor-independent') && html.includes('18'));
      assert.deepEqual(diagnostics, [], 'Supported installed SSR must not emit warnings');
    }
  } finally {
    console.error = originalError;
    console.warn = originalWarn;
  }
  console.log(JSON.stringify({ format, expectation, sameReact, rendered: expectation === 'single-react' }));
}

main().catch(error => { console.error(error); process.exitCode = 1; });
