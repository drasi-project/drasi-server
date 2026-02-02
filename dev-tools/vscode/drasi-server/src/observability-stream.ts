import { ObservabilityViewer } from './observability-viewer';

export class ObservabilityStream {
  private abortController: AbortController | undefined;

  async stream(url: string, viewer: ObservabilityViewer): Promise<void> {
    this.abortController = new AbortController();
    try {
      const response = await fetch(url, {
        method: 'GET',
        headers: { Accept: 'text/event-stream' },
        signal: this.abortController.signal,
      });
      if (!response.ok || !response.body) {
        viewer.appendError(`Stream failed: ${response.status} ${response.statusText}`);
        return;
      }
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      while (true) {
        const { value, done } = await reader.read();
        if (done) {
          break;
        }
        buffer += decoder.decode(value, { stream: true });
        let boundary = buffer.indexOf('\n\n');
        while (boundary >= 0) {
          const chunk = buffer.slice(0, boundary).trim();
          buffer = buffer.slice(boundary + 2);
          if (chunk) {
            this.handleEventChunk(chunk, viewer);
          }
          boundary = buffer.indexOf('\n\n');
        }
      }
    } catch (error) {
      if (!this.abortController?.signal.aborted) {
        viewer.appendError(`Stream error: ${error}`);
      }
    }
  }

  stop() {
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = undefined;
    }
  }

  private handleEventChunk(chunk: string, viewer: ObservabilityViewer) {
    const lines = chunk.split('\n');
    for (const line of lines) {
      if (!line.startsWith('data:')) {
        continue;
      }
      const payload = line.replace(/^data:\s?/, '');
      if (!payload) {
        continue;
      }
      try {
        viewer.appendItems([JSON.parse(payload)]);
      } catch (error) {
        viewer.appendRaw(payload);
      }
    }
  }
}
