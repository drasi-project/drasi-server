// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import { readFileSync } from 'node:fs';
import { relative } from 'node:path';
import { defineConfig } from 'vite';

export default defineConfig({
  resolve: { dedupe: ['react', 'react-dom'] },
  build: {
    rollupOptions: { input: ['index.html', 'hooks.html', 'showcase.html'] },
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
            const packageFile = /(?:node_modules\/@drasi\/react|dev-tools\/react)\/(dist\/.+)/.exec(id);
            return {
              id: packageFile ? `@drasi/react/${packageFile[1]}` : relative(process.cwd(), id).replaceAll('\\', '/'),
              // Inspect shipped maps, never source files, to identify a retained package chunk.
              sources: packageFile ? JSON.parse(readFileSync(`${id}.map`, 'utf8')).sources : [],
            };
          }),
        };
      }
      this.emitFile({ type: 'asset', fileName: 'build-graph.json', source: JSON.stringify(chunks, null, 2) + '\n' });
    },
  }],
});
