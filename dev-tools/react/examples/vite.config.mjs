// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import { readFileSync } from 'node:fs';
import { relative } from 'node:path';
import { defineConfig } from 'vite';
import { moduleIdentity } from './scripts/module-identity.mjs';

export default defineConfig({
  resolve: { dedupe: ['react', 'react-dom'] },
  build: {
    rollupOptions: { input: ['index.html', 'query-table.html', 'hooks.html', 'showcase.html'] },
  },
  plugins: [{
    name: 'record-built-import-graph',
    generateBundle(_options, bundle) {
      const chunks = {};
      for (const [filename, output] of Object.entries(bundle)) {
        if (output.type !== 'chunk') continue;
        chunks[filename] = {
          entry: output.facadeModuleId ? relative(process.cwd(), output.facadeModuleId) : null,
          imports: output.imports,
          dynamicImports: output.dynamicImports,
          css: [...(output.viteMetadata?.importedCss ?? [])],
          modules: Object.entries(output.modules).filter(([, info]) => info.renderedLength > 0).map(([id]) => {
            const module = moduleIdentity(id, process.cwd());
            return {
              id: module.id,
              // Inspect shipped maps, never source files, to identify a retained package chunk.
              sources: module.sourceMap ? JSON.parse(readFileSync(module.sourceMap, 'utf8')).sources : [],
            };
          }),
        };
      }
      this.emitFile({ type: 'asset', fileName: 'build-graph.json', source: JSON.stringify(chunks, null, 2) + '\n' });
    },
  }],
});
