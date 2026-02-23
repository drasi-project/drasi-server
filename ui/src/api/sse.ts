import type { ChangeEvent } from "./types";

export type SSECallback = (event: ChangeEvent) => void;

export function subscribeToQueryResults(
  queryId: string,
  onEvent: SSECallback,
  instanceId?: string,
): () => void {
  const path = instanceId
    ? `/api/v1/instances/${instanceId}/queries/${queryId}/attach`
    : `/api/v1/queries/${queryId}/attach`;

  const eventSource = new EventSource(path);

  eventSource.onmessage = (ev) => {
    try {
      const parsed = JSON.parse(ev.data);
      if (parsed.added) {
        for (const item of parsed.added) {
          onEvent({ type: "added", data: item });
        }
      }
      if (parsed.updated) {
        for (const item of parsed.updated) {
          onEvent({
            type: "updated",
            data: item.after ?? item,
            before: item.before,
          });
        }
      }
      if (parsed.deleted) {
        for (const item of parsed.deleted) {
          onEvent({ type: "deleted", data: item });
        }
      }
    } catch {
      // Skip malformed events
    }
  };

  return () => eventSource.close();
}

export function subscribeToComponentEvents(
  componentType: "sources" | "queries" | "reactions",
  componentId: string,
  onEvent: (data: unknown) => void,
  instanceId?: string,
): () => void {
  const path = instanceId
    ? `/api/v1/instances/${instanceId}/${componentType}/${componentId}/events/stream`
    : `/api/v1/${componentType}/${componentId}/events/stream`;

  const eventSource = new EventSource(path);

  eventSource.onmessage = (ev) => {
    try {
      onEvent(JSON.parse(ev.data));
    } catch {
      // Skip malformed events
    }
  };

  return () => eventSource.close();
}
