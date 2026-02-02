import * as vscode from 'vscode';

export type ObservabilityItem = Record<string, unknown> | string;

export class ObservabilityViewer {
  private output: vscode.OutputChannel;

  constructor(title: string) {
    this.output = vscode.window.createOutputChannel(title, { log: true });
  }

  show() {
    this.output.show();
  }

  appendHeader(title: string) {
    this.output.appendLine(`\n=== ${title} ===`);
  }

  appendItems(items: ObservabilityItem[]) {
    for (const item of items) {
      if (typeof item === 'string') {
        this.output.appendLine(item);
      } else {
        this.output.appendLine(JSON.stringify(item, null, 2));
      }
    }
  }

  appendRaw(raw: string) {
    this.output.appendLine(raw);
  }

  appendError(message: string) {
    this.output.appendLine(`ERROR: ${message}`);
  }

  dispose() {
    this.output.dispose();
  }
}
