import * as vscode from 'vscode';
import { getUri } from '../utilities/getUri';
import { getNonce } from '../utilities/getNonce';

export function queryResultsView(webview: vscode.Webview, extensionUri: vscode.Uri, initialStatus?: string) {
  const webviewUri = getUri(webview, extensionUri, ['out', 'webview.js']);
  const nonce = getNonce();

  return `
    <!DOCTYPE html>
    <html lang="en">
    <head>
      <meta charset="UTF-8">
      <meta name="viewport" content="width=device-width, initial-scale=1.0">
      <title>Query Results</title>
      <style>
        table { border-collapse: collapse; width: 100%; }
        th, td { border: 1px solid #ddd; padding: 8px; }
      </style>
    </head>
    <body>
      <div id="status">
      <h3>
        Status: <vscode-tag id="statusText">${initialStatus ?? 'Connecting'}</vscode-tag>
      </h3>
      </div>
      <div id="errors"></div>
      <vscode-divider></vscode-divider>
      <vscode-data-grid id="resultsTable" generate-header="sticky"></vscode-data-grid>
      <script type="module" nonce="${nonce}" src="${webviewUri}"></script>
      <script nonce="${nonce}">
        const resultsTable = document.getElementById('resultsTable');
        const statusText = document.getElementById('statusText');
        const errors = document.getElementById('errors');
        let resultValues = [];

        window.addEventListener('message', event => {
          const message = event.data;
          switch (message.kind) {
            case 'status':
              statusText.innerText = message.status;
              break;
            case 'error':
              const newItem = document.createElement('p');
              newItem.textContent = message.message;
              errors.appendChild(newItem);
              break;
            case 'results':
              resultValues = message.results || [];
              renderTable();
              break;
          }
        });

        function renderTable() {
          resultsTable.rowsData = Array.from(resultValues);
        }
      </script>
    </body>
    </html>
  `;
}
