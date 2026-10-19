export type QuerySseEvent =
  | { kind: 'snapshot'; rows: Record<string, unknown>[] }
  | { kind: 'changes'; payload: Record<string, unknown>; raw: string };

function isRow(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function parseQuerySseEvent(chunk: string): QuerySseEvent | undefined {
  let event = 'message';
  const data: string[] = [];
  for (const line of chunk.split('\n')) {
    if (line.startsWith('event:')) {
      event = line.slice(6).trimStart();
    } else if (line.startsWith('data:')) {
      data.push(line.slice(5).replace(/^ /, ''));
    }
  }
  const raw = data.join('\n');
  if (!raw.trim()) return undefined;
  const parsed: unknown = JSON.parse(raw);
  if (event === 'snapshot') {
    if (!Array.isArray(parsed) || !parsed.every(isRow)) {
      throw new Error('Query snapshot must contain an array of rows');
    }
    return { kind: 'snapshot', rows: parsed };
  }
  if (event !== 'message' || !isRow(parsed) || !Array.isArray(parsed.results)) {
    throw new Error('Invalid query change event');
  }
  return { kind: 'changes', payload: parsed, raw };
}

export function processSseBuffer(buffer: string): { events: string[]; remaining: string } {
  const events: string[] = [];
  let remaining = buffer.replace(/\r\n/g, '\n');
  let boundary = remaining.indexOf('\n\n');
  while (boundary >= 0) {
    const chunk = remaining.slice(0, boundary).trim();
    remaining = remaining.slice(boundary + 2);
    if (chunk) events.push(chunk);
    boundary = remaining.indexOf('\n\n');
  }
  return { events, remaining };
}
