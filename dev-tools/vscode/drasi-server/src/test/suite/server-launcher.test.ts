import * as assert from 'assert';
import * as vscode from 'vscode';
import { ServerLauncher } from '../../server-launcher';

const BINARY_PATH_SETTING = 'drasiServer.binaryPath';

async function clearBinaryPathSetting() {
  await vscode.workspace
    .getConfiguration()
    .update(BINARY_PATH_SETTING, undefined, vscode.ConfigurationTarget.Global);
}

suite('ServerLauncher settings scope', () => {
  setup(async () => {
    await clearBinaryPathSetting();
  });

  teardown(async () => {
    await clearBinaryPathSetting();
  });

  test('configureBinary writes binaryPath to Global, not Workspace', async function () {
    if (!vscode.workspace.workspaceFolders || vscode.workspace.workspaceFolders.length === 0) {
      this.skip();
    }

    const selectedPath = '/tmp/drasi-server-test-binary';
    const originalOpenDialog = vscode.window.showOpenDialog;
    const originalInfoMessage = vscode.window.showInformationMessage;

    try {
      (vscode.window as any).showOpenDialog = async () => [vscode.Uri.file(selectedPath)];
      (vscode.window as any).showInformationMessage = async () => undefined;

      const launcher = new ServerLauncher();
      await launcher.configureBinary();

      const inspect = vscode.workspace.getConfiguration().inspect<string>(BINARY_PATH_SETTING);
      assert.strictEqual(inspect?.globalValue, selectedPath);
      assert.strictEqual(
        inspect?.workspaceValue,
        undefined,
        'binaryPath must not be workspace-scoped'
      );
    } finally {
      (vscode.window as any).showOpenDialog = originalOpenDialog;
      (vscode.window as any).showInformationMessage = originalInfoMessage;
    }
  });
});
