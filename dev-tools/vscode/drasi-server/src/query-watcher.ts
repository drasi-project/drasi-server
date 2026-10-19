import * as vscode from 'vscode';
import { DrasiClient } from './drasi-client';
import { queryResultsView } from './webview/query-results-view';
import { parseQuerySseEvent, processSseBuffer } from './query-sse';

export class QueryWatcher {
  private log: vscode.OutputChannel;
  private resultsPanel: vscode.WebviewPanel | undefined;
  private extensionUri: vscode.Uri;
  private drasiClient: DrasiClient;
  private queryId: string;
  private timer: NodeJS.Timeout | undefined;
  private streaming = false;
  private stopped = false;
  private abortController: AbortController | undefined;

  constructor(queryId: string, extensionUri: vscode.Uri, drasiClient: DrasiClient) {
    this.log = vscode.window.createOutputChannel(`Query Watch: ${queryId}`, { log: true });
    this.extensionUri = extensionUri;
    this.queryId = queryId;
    this.drasiClient = drasiClient;
  }

  async start() {
    this.log.show();
    this.createResultsPanel();
    await this.startStreaming();
    if (this.stopped) return;
    await this.fetchAndRender();
    if (!this.stopped) {
      this.timer = setInterval(() => {
        this.fetchAndRender();
      }, 5000);
    }
  }

  private async fetchAndRender() {
    try {
      const results = await this.drasiClient.getQueryResults(this.queryId);
      if (this.stopped) return;
      this.resultsPanel?.webview.postMessage({ kind: 'status', status: 'Watching' });
      this.resultsPanel?.webview.postMessage({ kind: 'results', results });
    } catch (error) {
      if (this.stopped) return;
      const message = String(error);
      this.log.appendLine(message);
      this.resultsPanel?.webview.postMessage({ kind: 'error', message });
    }
  }

  private createResultsPanel() {
    this.resultsPanel = vscode.window.createWebviewPanel(
      'queryWatch',
      `Query Watch: ${this.queryId}`,
      vscode.ViewColumn.One,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [vscode.Uri.joinPath(this.extensionUri, 'out')],
      }
    );

    this.resultsPanel.webview.html = queryResultsView(this.resultsPanel.webview, this.extensionUri, 'Starting');

    this.resultsPanel.onDidDispose(() => {
      this.resultsPanel = undefined;
      this.stop();
    });
  }

  stop() {
    if (this.stopped) return;
    this.stopped = true;
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = undefined;
    }
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = undefined;
    }
    this.log.dispose();
  }

  private async startStreaming(): Promise<void> {
    let url: string;
    try {
      url = this.drasiClient.getQueryAttachUrl(this.queryId);
    } catch (error) {
      this.log.appendLine(String(error));
      return;
    }
    const controller = new AbortController();
    this.abortController = controller;
    let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
    try {
      const response = await fetch(url, {
        method: 'GET',
        headers: { Accept: 'text/event-stream' },
        signal: controller.signal,
      });
      if (!response.ok || !response.body) {
        this.log.appendLine(`Attach failed: ${response.status} ${response.statusText}`);
        this.streaming = false;
        return;
      }
      this.resultsPanel?.webview.postMessage({ kind: 'status', status: 'Loading snapshot' });
      reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      while (true) {
        const { value, done } = await reader.read();
        if (done) {
          break;
        }
        buffer += decoder.decode(value, { stream: true });
        const parsed = processSseBuffer(buffer);
        buffer = parsed.remaining;
        for (const chunk of parsed.events) {
          if (this.stopped) return;
          this.handleSseChunk(chunk);
        }
      }
    } catch (error) {
      if (!controller.signal.aborted) {
        this.log.appendLine(`Attach stream error: ${error}`);
        this.resultsPanel?.webview.postMessage({ kind: 'error', message: String(error) });
      }
    } finally {
      controller.abort();
      reader?.releaseLock();
      this.streaming = false;
      this.abortController = undefined;
    }
  }

  private handleSseChunk(chunk: string) {
    const event = parseQuerySseEvent(chunk);
    if (!event) return;
    if (event.kind === 'snapshot') {
      if (this.streaming) throw new Error('Duplicate query snapshot');
      this.resultsPanel?.webview.postMessage({ kind: 'results', results: event.rows });
      this.resultsPanel?.webview.postMessage({ kind: 'status', status: 'Streaming' });
      this.streaming = true;
    } else {
      if (!this.streaming) throw new Error('Received query changes before the snapshot');
      this.resultsPanel?.webview.postMessage({
        kind: 'stream', payload: event.payload, raw: event.raw,
      });
    }
  }
}
