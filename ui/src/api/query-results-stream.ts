import { z } from "zod";
import type { QueryResultRow } from "./types";

const row = z.record(z.unknown());
const change = z.discriminatedUnion("type", [
  z.object({ type: z.literal("ADD"), data: row }),
  z.object({ type: z.literal("DELETE"), data: row }),
  z.object({
    type: z.literal("UPDATE"),
    before: row.nullable().optional(),
    after: row.nullable().optional(),
  }),
  z.object({
    type: z.literal("aggregation"),
    before: row.nullable().optional(),
    after: row.nullable().optional(),
  }),
  z.object({ type: z.literal("noop") }),
]);
const payload = z.object({ results: z.array(change) });
const snapshot = z.array(row);

export type StreamPayload = z.infer<typeof payload>;

interface StreamStatus {
  loading: boolean;
  streaming: boolean;
  error: string | null;
}

interface Handlers {
  onSnapshot(rows: QueryResultRow[]): void;
  onChange(change: StreamPayload): void;
  onStatus(status: StreamStatus): void;
}

export function subscribeQueryResults(path: string, handlers: Handlers): () => void {
  const source = new EventSource(path);
  let active = true;
  let ready = false;

  const fail = (error: unknown) => {
    active = false;
    source.close();
    handlers.onStatus({
      loading: false,
      streaming: false,
      error: `Invalid query result stream: ${String(error)}`,
    });
  };

  source.onopen = () => {
    if (!active) return;
    ready = false;
    handlers.onStatus({ loading: true, streaming: false, error: null });
  };
  source.addEventListener("snapshot", (event) => {
    if (!active) return;
    try {
      if (ready) throw new Error("Received another snapshot without reconnecting");
      handlers.onSnapshot(snapshot.parse(JSON.parse(event.data)));
      ready = true;
      handlers.onStatus({ loading: false, streaming: true, error: null });
    } catch (error) {
      fail(error);
    }
  });
  source.onmessage = (event) => {
    if (!active) return;
    try {
      if (!ready) throw new Error("Received changes before the initial snapshot");
      handlers.onChange(payload.parse(JSON.parse(event.data)));
    } catch (error) {
      fail(error);
    }
  };
  source.onerror = () => {
    if (!active) return;
    ready = false;
    handlers.onStatus({
      loading: true,
      streaming: false,
      error: "Query stream disconnected; waiting for a fresh snapshot.",
    });
  };

  return () => {
    active = false;
    source.close();
  };
}
