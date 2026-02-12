import { ObservabilityViewer, LogTerminalViewer } from './observability-viewer';
import { ComponentEvent, LogMessage } from './models/common';

export type StreamMode = 'events' | 'logs';

export class ObservabilityStream {
  private abortController: AbortController | undefined;

  async stream(url: string, viewer: ObservabilityViewer, mode: StreamMode = 'events'): Promise<void> {
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
        buffer = buffer.replace(/\r\n/g, '\n');
        let boundary = buffer.indexOf('\n\n');
        while (boundary >= 0) {
          const chunk = buffer.slice(0, boundary).trim();
          buffer = buffer.slice(boundary + 2);
          if (chunk) {
            this.handleEventChunk(chunk, viewer, mode);
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

  async streamLogs(url: string, viewer: LogTerminalViewer): Promise<void> {
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
        buffer = buffer.replace(/\r\n/g, '\n');
        let boundary = buffer.indexOf('\n\n');
        while (boundary >= 0) {
          const chunk = buffer.slice(0, boundary).trim();
          buffer = buffer.slice(boundary + 2);
          if (chunk) {
            this.handleLogChunk(chunk, viewer);
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

  private handleEventChunk(chunk: string, viewer: ObservabilityViewer, mode: StreamMode) {
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
        const parsed = JSON.parse(payload);
        if (mode === 'logs') {
          viewer.appendLogMessages([parsed as LogMessage]);
        } else {
          viewer.appendItems([parsed]);
        }
      } catch {
        viewer.appendRaw(payload);
      }
    }
  }

  private handleLogChunk(chunk: string, viewer: LogTerminalViewer) {
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
        const parsed = JSON.parse(payload) as LogMessage;
        viewer.appendLogMessage(parsed);
      } catch {
        viewer.appendRaw(payload);
      }
    }
  }

  async streamEvents(url: string, viewer: LogTerminalViewer): Promise<void> {
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
        buffer = buffer.replace(/\r\n/g, '\n');
        let boundary = buffer.indexOf('\n\n');
        while (boundary >= 0) {
          const chunk = buffer.slice(0, boundary).trim();
          buffer = buffer.slice(boundary + 2);
          if (chunk) {
            this.handleTerminalEventChunk(chunk, viewer);
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

  private handleTerminalEventChunk(chunk: string, viewer: LogTerminalViewer) {
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
        const parsed = JSON.parse(payload) as ComponentEvent;
        viewer.appendEvent(parsed);
      } catch {
        viewer.appendRaw(payload);
      }
    }
  }
}
