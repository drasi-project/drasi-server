// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { readFile, stat } from 'node:fs/promises';
import { dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../../../', import.meta.url));
const documents = [
  'dev-tools/react/README.md', 'dev-tools/react/CHANGELOG.md',
  'examples/react/README.md', 'examples/trading/TESTING.md',
];

function prose(markdown) {
  const lines = [];
  let fence = '';
  for (const line of markdown.split(/\r?\n/)) {
    const marker = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
    if (fence) {
      if (marker && marker[1][0] === fence[0] && marker[1].length >= fence.length && !marker[2].trim()) fence = '';
    } else if (marker) fence = marker[1];
    else lines.push(line);
  }
  return lines.join('\n');
}

export function markdownAnchors(markdown) {
  const text = prose(markdown), anchors = new Set(), counts = new Map();
  for (const match of text.matchAll(/^ {0,3}#{1,6} +(.+?)(?: +#+)? *$/gm)) {
    const name = match[1].replace(/<[^>]*>/g, '').toLowerCase()
      .replace(/[^\p{L}\p{N}_ -]/gu, '').replace(/ /g, '-');
    const count = counts.get(name) ?? 0;
    anchors.add(count ? `${name}-${count}` : name);
    counts.set(name, count + 1);
  }
  for (const match of text.matchAll(/<a\s+(?:id|name)=["']([^"']+)["'][^>]*>/g)) anchors.add(match[1]);
  return anchors;
}

export function relativeMarkdownLinks(markdown) {
  return [...prose(markdown).matchAll(/(?<!!)\[[^\]\n]+\]\(([^()\s]+)(?:\s+"[^"]*")?\)/g)]
    .map(match => match[1])
    .filter(target => !/^[a-z][a-z0-9+.-]*:/i.test(target) && !target.startsWith('//'));
}

export async function checkDocumentationLinks(overrides = {}) {
  const contents = new Map(Object.entries(overrides).map(([file, text]) => [resolve(root, file), text]));
  const load = async file => {
    if (!contents.has(file)) contents.set(file, await readFile(file, 'utf8'));
    return contents.get(file);
  };
  const checked = [];
  for (const name of documents) {
    const source = resolve(root, name);
    for (const target of relativeMarkdownLinks(await load(source))) {
      const [path, fragment] = target.split('#');
      const file = path ? resolve(dirname(source), decodeURIComponent(path)) : source;
      const local = relative(root, file);
      assert(local !== '..' && !local.startsWith(`..${sep}`), `Documentation link escapes repository: ${name} -> ${target}`);
      const info = await stat(file);
      assert(info.isFile() || info.isDirectory(), `Invalid documentation target: ${name} -> ${target}`);
      if (fragment) {
        assert(info.isFile(), `Directory fragment needs an explicit document: ${name} -> ${target}`);
        assert(markdownAnchors(await load(file)).has(decodeURIComponent(fragment)),
          `Missing documentation anchor: ${name} -> ${target}`);
      }
      checked.push({ source: name, target });
    }
  }
  return { documents, checked };
}
