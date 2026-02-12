import * as yaml from 'yaml';

const DRASI_API_PREFIX = 'drasi.io/';

/**
 * Parses a YAML document and extracts the API version from a top-level
 * `apiVersion` field that starts with `drasi.io/`.
 * Returns the version segment (e.g. `"v1"`) or `undefined` if not found.
 */
export function parseDrasiApiVersion(text: string): string | undefined {
  try {
    const docs = yaml.parseAllDocuments(text);
    for (const doc of docs) {
      const obj = doc.toJS();
      if (obj && typeof obj === 'object' && typeof obj.apiVersion === 'string') {
        const value: string = obj.apiVersion;
        if (value.startsWith(DRASI_API_PREFIX)) {
          return value.slice(DRASI_API_PREFIX.length);
        }
      }
    }
  } catch {
    // not valid yaml
  }
  return undefined;
}

/**
 * Checks whether a YAML text represents a drasi file
 * (contains a top-level `apiVersion` starting with `drasi.io/`).
 */
export function isDrasiYaml(text: string): boolean {
  return parseDrasiApiVersion(text) !== undefined;
}
