import * as vscode from 'vscode';
import * as yaml from 'yaml';

const BINARY_PATH_SETTING = 'drasiServer.binaryPath';

export class ServerLauncher {
  async launch(configFilePath: string, defaultPort?: number) {
    const binaryPath = await this.resolveBinaryPath();
    if (!binaryPath) {
      return;
    }

    const portStr = await vscode.window.showInputBox({
      prompt: 'Port number for the Drasi Server',
      value: defaultPort !== undefined ? String(defaultPort) : '8080',
      ignoreFocusOut: true,
      validateInput: (value) => {
        const num = Number(value);
        if (!Number.isInteger(num) || num < 1 || num > 65535) {
          return 'Enter a valid port number (1-65535)';
        }
        return undefined;
      },
    });

    if (portStr === undefined) {
      return;
    }

    const port = Number(portStr);
    const terminal = vscode.window.createTerminal({
      name: `Drasi Server :${port}`,
      shellPath: binaryPath,
      shellArgs: ['--config', configFilePath, '--port', String(port)],
    });
    terminal.show();
  }

  async configureBinary() {
    const current = vscode.workspace.getConfiguration().get<string>(BINARY_PATH_SETTING);

    const result = await vscode.window.showOpenDialog({
      canSelectFiles: true,
      canSelectFolders: false,
      canSelectMany: false,
      openLabel: 'Select drasi-server binary',
      defaultUri: current ? vscode.Uri.file(current) : undefined,
    });

    if (!result || result.length === 0) {
      return;
    }

    const selected = result[0].fsPath;
    const target = vscode.workspace.workspaceFolders
      ? vscode.ConfigurationTarget.Workspace
      : vscode.ConfigurationTarget.Global;
    await vscode.workspace.getConfiguration().update(BINARY_PATH_SETTING, selected, target);
    vscode.window.showInformationMessage(`Drasi Server binary set to: ${selected}`);
  }

  private async resolveBinaryPath(): Promise<string | undefined> {
    const configured = vscode.workspace.getConfiguration().get<string>(BINARY_PATH_SETTING);
    if (configured) {
      return configured;
    }

    const result = await vscode.window.showOpenDialog({
      canSelectFiles: true,
      canSelectFolders: false,
      canSelectMany: false,
      openLabel: 'Select drasi-server binary',
    });

    if (!result || result.length === 0) {
      return undefined;
    }

    const selected = result[0].fsPath;
    const target = vscode.workspace.workspaceFolders
      ? vscode.ConfigurationTarget.Workspace
      : vscode.ConfigurationTarget.Global;
    await vscode.workspace.getConfiguration().update(BINARY_PATH_SETTING, selected, target);
    return selected;
  }
}

/**
 * Extracts the port number from a parsed drasi config document.
 */
export function extractPort(text: string): number | undefined {
  try {
    const docs = yaml.parseAllDocuments(text);
    for (const doc of docs) {
      const obj = doc.toJS();
      if (obj && typeof obj === 'object' && obj.port !== undefined) {
        const port = Number(obj.port);
        if (Number.isInteger(port) && port > 0 && port <= 65535) {
          return port;
        }
      }
    }
  } catch {
    // ignore
  }
  return undefined;
}
