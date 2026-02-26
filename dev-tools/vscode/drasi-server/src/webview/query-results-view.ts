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
        #rawStream {
          font-family: var(--vscode-editor-font-family, monospace);
          font-size: var(--vscode-editor-font-size, 12px);
          max-height: 60vh;
          overflow: auto;
        }
        .raw-item {
          border: 1px solid var(--vscode-panel-border);
          border-radius: 4px;
          margin-bottom: 8px;
          background: var(--vscode-editor-background);
          color: var(--vscode-editor-foreground);
          width: 100%;
          box-sizing: border-box;
        }
        .raw-item > summary {
          cursor: pointer;
          padding: 6px 10px;
          background: var(--vscode-panel-background);
          user-select: none;
          display: block;
        }
        .raw-item[open] > summary {
          border-bottom: 1px solid var(--vscode-panel-border);
        }
        .raw-json {
          margin: 0;
          padding: 8px 10px;
          white-space: pre;
          overflow-x: auto;
          width: 100%;
          box-sizing: border-box;
        }
        .json-key { color: var(--vscode-symbolIcon-propertyForeground, var(--vscode-editor-foreground)); }
        .json-string { color: var(--vscode-editor-stringForeground, var(--vscode-editor-foreground)); }
        .json-number { color: var(--vscode-editor-numberForeground, var(--vscode-editor-foreground)); }
        .json-boolean { color: var(--vscode-editor-booleanForeground, var(--vscode-editor-foreground)); }
        .json-null { color: var(--vscode-editor-booleanForeground, var(--vscode-editor-foreground)); }
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
      <vscode-panels activeid="results-tab">
        <vscode-panel-tab id="results-tab">Results</vscode-panel-tab>
        <vscode-panel-tab id="raw-tab">Raw Stream</vscode-panel-tab>
        <vscode-panel-view>
          <vscode-data-grid id="resultsTable" generate-header="sticky"></vscode-data-grid>
        </vscode-panel-view>
        <vscode-panel-view>
          <div id="rawStream"></div>
        </vscode-panel-view>
      </vscode-panels>
      <script type="module" nonce="${nonce}" src="${webviewUri}"></script>
      <script nonce="${nonce}">
        const resultsTable = document.getElementById('resultsTable');
        const statusText = document.getElementById('statusText');
        const errors = document.getElementById('errors');
        const rawStream = document.getElementById('rawStream');
        let resultValues = [];
        let rawMessages = [];

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
              resultValues = dedupeResults(message.results || []);
              renderTable();
              break;
            case 'stream':
              appendRaw(message.raw);
              applyStreamUpdate(message.payload);
              renderTable();
              break;
            case 'raw':
              appendRaw(message.raw);
              break;
          }
        });

        function renderTable() {
          resultsTable.rowsData = Array.from(resultValues);
        }

        function appendRaw(raw) {
          if (!raw) {
            return;
          }
          const item = createRawItem(raw);
          rawMessages.push(item);
          rawStream.appendChild(item);
          if (rawMessages.length > 500) {
            const removed = rawMessages.shift();
            if (removed && removed.parentElement) {
              removed.parentElement.removeChild(removed);
            }
          }
          rawStream.scrollTop = rawStream.scrollHeight;
        }

        function createRawItem(raw) {
          const details = document.createElement('details');
          details.className = 'raw-item';
          const summary = document.createElement('summary');
          summary.textContent = new Date().toISOString();
          const pre = document.createElement('pre');
          pre.className = 'raw-json';
          const code = document.createElement('code');
          const formatted = formatRawJson(raw);
          if (formatted.isJson) {
            code.innerHTML = syntaxHighlight(formatted.value);
          } else {
            code.textContent = formatted.value;
          }
          pre.appendChild(code);
          details.appendChild(summary);
          details.appendChild(pre);
          return details;
        }

        function formatRawJson(raw) {
          const rawText = String(raw).trimEnd();
          try {
            const parsed = JSON.parse(rawText);
            return { isJson: true, value: JSON.stringify(parsed, null, 2) };
          } catch (error) {
            return { isJson: false, value: rawText };
          }
        }

        function syntaxHighlight(json) {
          const escaped = escapeHtml(json);
          return escaped.replace(
            /("(\\u[a-fA-F0-9]{4}|\\[^u]|[^\\"])*"(\\s*:)?|\\b(true|false|null)\\b|-?\\d+(?:\\.\\d*)?(?:[eE][+\\-]?\\d+)?)/g,
            (match) => {
              if (/^"/.test(match)) {
                if (/:$/.test(match)) {
                  return '<span class="json-key">' + match + '</span>';
                }
                return '<span class="json-string">' + match + '</span>';
              }
              if (/true|false/.test(match)) {
                return '<span class="json-boolean">' + match + '</span>';
              }
              if (/null/.test(match)) {
                return '<span class="json-null">' + match + '</span>';
              }
              return '<span class="json-number">' + match + '</span>';
            }
          );
        }

        function escapeHtml(value) {
          return value
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
        }

        function applyStreamUpdate(payload) {
          if (!payload || !Array.isArray(payload.results)) {
            return;
          }
          payload.results.forEach((entry) => {
            switch (entry.type) {
              case 'ADD':
                upsertRow(entry.data);
                break;
              case 'DELETE':
                removeRow(entry.data);
                break;
              case 'UPDATE':
                replaceRow(entry.before, entry.after);
                break;
              case 'aggregation':
                if (entry.after) {
                  replaceRow(entry.before, entry.after);
                }
                break;
              case 'noop':
                break;
            }
          });
        }

        function dedupeResults(values) {
          const seen = new Set();
          const deduped = [];
          values.forEach((item) => {
            const key = stableKey(item);
            if (seen.has(key)) {
              return;
            }
            seen.add(key);
            deduped.push(item);
          });
          return deduped;
        }

        function upsertRow(target) {
          const targetKey = stableKey(target);
          const index = resultValues.findIndex((item) => stableKey(item) === targetKey);
          if (index >= 0) {
            resultValues[index] = target;
          } else {
            resultValues.push(target);
          }
        }

        function removeRow(target) {
          const targetKey = stableKey(target);
          const index = resultValues.findIndex((item) => stableKey(item) === targetKey);
          if (index >= 0) {
            resultValues.splice(index, 1);
          }
        }

        function replaceRow(before, after) {
          const beforeKey = stableKey(before);
          const index = resultValues.findIndex((item) => stableKey(item) === beforeKey);
          if (index >= 0 && after) {
            resultValues[index] = after;
          } else if (after) {
            const afterKey = stableKey(after);
            const afterIndex = resultValues.findIndex((item) => stableKey(item) === afterKey);
            if (afterIndex >= 0) {
              resultValues[afterIndex] = after;
            } else {
              resultValues.push(after);
            }
          }
        }

        function stableKey(value) {
          return JSON.stringify(normalizeValue(value));
        }

        function normalizeValue(value) {
          if (Array.isArray(value)) {
            return value.map((item) => normalizeValue(item));
          }
          if (value && typeof value === 'object') {
            const keys = Object.keys(value).sort();
            const normalized = {};
            keys.forEach((key) => {
              normalized[key] = normalizeValue(value[key]);
            });
            return normalized;
          }
          return value;
        }
      </script>
    </body>
    </html>
  `;
}
