// Copyright 2026 The Drasi Authors. Licensed under the Apache License, Version 2.0.
import assert from 'node:assert/strict';
import { readFile, realpath, stat } from 'node:fs/promises';
import { dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { documentationFiles } from './documents.mjs';

const root = fileURLToPath(new URL('../../../../', import.meta.url));
const documents = [
  ...documentationFiles.map(file => `dev-tools/react/${file}`),
  'dev-tools/react/examples/README.md', 'examples/react/README.md', 'examples/trading/TESTING.md',
];
const repositoryLink = /^https:\/\/github\.com\/drasi-project\/drasi-server\/(?:blob|tree)\/main\//;

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

function markdownLinks(markdown) {
  return [...prose(markdown).matchAll(/(?<!!)\[[^\]\n]+\]\(([^()\s]+)(?:\s+"[^"]*")?\)/g)]
    .map(match => match[1]);
}

export function relativeMarkdownLinks(markdown) {
  return markdownLinks(markdown)
    .filter(target => !/^[a-z][a-z0-9+.-]*:/i.test(target) && !target.startsWith('//'));
}

export async function checkInstalledDocumentationLinks(packageRoot, contents) {
  const packagePath = await realpath(packageRoot);
  const checked = [];
  for (const name of documentationFiles) {
    assert.equal(typeof contents[name], 'string', `Missing installed document: ${name}`);
  }
  for (const [name, markdown] of Object.entries(contents)) {
    for (const target of relativeMarkdownLinks(markdown)) {
      const [path, fragment] = target.split('#');
      const source = resolve(packagePath, name);
      const file = path ? resolve(dirname(source), decodeURIComponent(path)) : source;
      const local = relative(packagePath, file);
      assert(local !== '..' && !local.startsWith(`..${sep}`),
        `Installed documentation link escapes package: ${name} -> ${target}`);
      const resolved = await realpath(file);
      assert.equal(resolved, file, `Installed documentation must not resolve a source alias: ${target}`);
      assert((await stat(file)).isFile(), `Installed link must name a shipped file: ${target}`);
      if (fragment) {
        const text = contents[local] ?? await readFile(file, 'utf8');
        assert(markdownAnchors(text).has(decodeURIComponent(fragment)),
          `Missing installed documentation anchor: ${name} -> ${target}`);
      }
      checked.push({ source: name, target });
    }
  }
  return checked;
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
    const markdown = await load(source);
    const targets = [...relativeMarkdownLinks(markdown), ...markdownLinks(markdown).filter(target => repositoryLink.test(target))];
    for (const target of targets) {
      const fromRepository = repositoryLink.test(target);
      const [path, fragment] = target.replace(repositoryLink, '').split('#');
      const file = path ? resolve(fromRepository ? root : dirname(source), decodeURIComponent(path)) : source;
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
