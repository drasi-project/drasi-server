import * as vscode from 'vscode';
import { DrasiClient } from './drasi-client';
import { queryResultsView } from './webview/query-results-view';

export class QueryDebugger {
  private log: vscode.OutputChannel;
  private resultsPanel: vscode.WebviewPanel | undefined;
  private extensionUri: vscode.Uri;
  private drasiClient: DrasiClient;
  private queryId: string;

  constructor(queryId: string, extensionUri: vscode.Uri, drasiClient: DrasiClient) {
    this.log = vscode.window.createOutputChannel(`Query Debug: ${queryId}`, { log: true });
    this.extensionUri = extensionUri;
    this.queryId = queryId;
    this.drasiClient = drasiClient;
  }

  async start() {
    this.log.show();
    this.createResultsPanel();
    try {
      const results = await this.drasiClient.getQueryResults(this.queryId);
      this.resultsPanel?.webview.postMessage({ kind: 'status', status: 'Fetched' });
      this.resultsPanel?.webview.postMessage({ kind: 'results', results });
    } catch (error) {
      const message = String(error);
      this.log.appendLine(message);
      this.resultsPanel?.webview.postMessage({ kind: 'error', message });
      vscode.window.showErrorMessage(message);
    }
  }

  private createResultsPanel() {
    this.resultsPanel = vscode.window.createWebviewPanel(
      'queryResults',
      `Query Results: ${this.queryId}`,
      vscode.ViewColumn.One,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [vscode.Uri.joinPath(this.extensionUri, 'out')],
      }
    );

    this.resultsPanel.webview.html = queryResultsView(this.resultsPanel.webview, this.extensionUri, 'Fetching');

    this.resultsPanel.onDidDispose(() => {
      this.resultsPanel = undefined;
      this.stop();
    });
  }

  stop() {
    this.log.dispose();
  }
}
