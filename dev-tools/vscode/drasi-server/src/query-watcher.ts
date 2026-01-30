import * as vscode from 'vscode';
import { DrasiClient } from './drasi-client';
import { queryResultsView } from './webview/query-results-view';

export class QueryWatcher {
  private log: vscode.OutputChannel;
  private resultsPanel: vscode.WebviewPanel | undefined;
  private extensionUri: vscode.Uri;
  private drasiClient: DrasiClient;
  private queryId: string;
  private timer: NodeJS.Timeout | undefined;
  private streaming = false;
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
    await this.fetchAndRender();
    if (await this.startStreaming()) {
      return;
    }
    if (!this.streaming) {
      this.timer = setInterval(() => {
        this.fetchAndRender();
      }, 5000);
    }
  }

  private async fetchAndRender() {
    try {
      const results = await this.drasiClient.getQueryResults(this.queryId);
      this.resultsPanel?.webview.postMessage({ kind: 'status', status: 'Watching' });
      this.resultsPanel?.webview.postMessage({ kind: 'results', results });
    } catch (error) {
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

  private async startStreaming(): Promise<boolean> {
    let url: string;
    try {
      url = this.drasiClient.getQueryAttachUrl(this.queryId);
    } catch (error) {
      this.log.appendLine(String(error));
      return false;
    }
    this.abortController = new AbortController();
    try {
      const response = await fetch(url, {
        method: 'GET',
        headers: { Accept: 'application/x-ndjson' },
        signal: this.abortController.signal,
      });
      if (!response.ok || !response.body) {
        this.log.appendLine(`Attach failed: ${response.status} ${response.statusText}`);
        this.streaming = false;
        return false;
      }
      this.streaming = true;
      this.resultsPanel?.webview.postMessage({ kind: 'status', status: 'Streaming' });
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      while (true) {
        const { value, done } = await reader.read();
        if (done) {
          break;
        }
        buffer += decoder.decode(value, { stream: true });
        let newlineIndex = buffer.indexOf('\n');
        while (newlineIndex >= 0) {
          const line = buffer.slice(0, newlineIndex).trim();
          buffer = buffer.slice(newlineIndex + 1);
          if (line) {
            try {
              const payload = JSON.parse(line);
              this.resultsPanel?.webview.postMessage({ kind: 'stream', payload, raw: line });
            } catch (error) {
              this.resultsPanel?.webview.postMessage({ kind: 'raw', raw: line });
              this.log.appendLine(`Failed to parse stream payload: ${error}`);
            }
          }
          newlineIndex = buffer.indexOf('\n');
        }
      }
    } catch (error) {
      if (!this.abortController?.signal.aborted) {
        this.log.appendLine(`Attach stream error: ${error}`);
      }
      return false;
    } finally {
      this.streaming = false;
      this.abortController = undefined;
    }
    return true;
  }
}
