import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const hosting = process.argv[2] ?? 'embedded';
assert.ok(['embedded', 'server'].includes(hosting), 'Expected embedded or server hosting layout');
const example = resolve(dirname(fileURLToPath(import.meta.url)), '../..', hosting);
const destination = join(example, '.build/npm-cache');
const cache = execFileSync('npm', ['config', 'get', 'cache'], { encoding: 'utf8' }).trim();
const registry = new URL(process.env.NPM_REGISTRY ?? 'https://registry.npmjs.org/');
assert.ok(registry.protocol === 'https:' && !registry.username && !registry.password && !registry.search,
  'Docker registry configuration must be an HTTPS URL without embedded credentials');
await mkdir(destination, { recursive: true });
const integrities = new Set();
for (const file of ['../shared/ui/package-lock.json', '../../../ui/package-lock.json',
  '.build/react-source/dev-tools/react/package-lock.json']) {
  const lock = JSON.parse(await readFile(join(example, file), 'utf8'));
  for (const entry of Object.values(lock.packages)) {
    if (/^(sha512|sha1)-/.test(entry.integrity ?? '') && entry.resolved?.startsWith('https://')) {
      integrities.add(entry.integrity);
    }
  }
}
let copied = 0;
for (const integrity of integrities) {
  const [algorithm, encoded] = integrity.split('-');
  const digest = Buffer.from(encoded, 'base64').toString('hex');
  assert.equal(digest.length, algorithm === 'sha512' ? 128 : 40);
  const source = join(cache, '_cacache/content-v2', algorithm, digest.slice(0, 2), digest.slice(2, 4), digest.slice(4));
  let data;
  try { data = await readFile(source); }
  catch (error) { if (error.code === 'ENOENT') continue; throw error; }
  assert.equal(`${algorithm}-${createHash(algorithm).update(data).digest('base64')}`, integrity,
    'Cached dependency does not match the committed lockfile integrity');
  await writeFile(join(destination, `${digest}.tgz`), data);
  copied++;
}
console.log(`Staged ${copied}/${integrities.size} integrity-verified npm archives; missing packages still require the registry.`);
