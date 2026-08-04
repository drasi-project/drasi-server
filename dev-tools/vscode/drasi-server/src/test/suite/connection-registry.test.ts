import * as assert from 'assert';
import * as vscode from 'vscode';
import { ConnectionRegistry, ServerConnectionConfig } from '../../sdk/config';

const SECTION = 'drasiServer';

async function clearConnectionSettings() {
  const config = vscode.workspace.getConfiguration(SECTION);
  await config.update('connections', undefined, vscode.ConfigurationTarget.Global);
  await config.update('currentConnectionId', undefined, vscode.ConfigurationTarget.Global);
}

suite('ConnectionRegistry settings scope', () => {
  setup(async () => {
    await clearConnectionSettings();
  });

  teardown(async () => {
    await clearConnectionSettings();
  });

  test('writes connections and currentConnectionId to Global, not Workspace', async function () {
    if (!vscode.workspace.workspaceFolders || vscode.workspace.workspaceFolders.length === 0) {
      this.skip();
    }

    const registry = new ConnectionRegistry();
    const connection = await registry.addConnection('Test Server', 'http://example.test:9090');

    const config = vscode.workspace.getConfiguration(SECTION);
    const connectionsInspect = config.inspect<ServerConnectionConfig[]>('connections');
    const currentIdInspect = config.inspect<string>('currentConnectionId');

    assert.ok(connectionsInspect?.globalValue, 'connections should be stored in global/user settings');
    assert.strictEqual(
      connectionsInspect?.workspaceValue,
      undefined,
      'connections must not be workspace-scoped'
    );
    assert.strictEqual(connectionsInspect?.globalValue?.[0]?.url, 'http://example.test:9090');

    assert.strictEqual(currentIdInspect?.globalValue, connection.id);
    assert.strictEqual(
      currentIdInspect?.workspaceValue,
      undefined,
      'currentConnectionId must not be workspace-scoped'
    );
  });

  test('ensureDefaultConnection does not write workspace settings', async () => {
    const registry = new ConnectionRegistry();
    await registry.ensureDefaultConnection();

    const config = vscode.workspace.getConfiguration(SECTION);
    const connectionsInspect = config.inspect<ServerConnectionConfig[]>('connections');
    const currentIdInspect = config.inspect<string>('currentConnectionId');

    assert.ok(connectionsInspect?.globalValue && connectionsInspect.globalValue.length > 0);
    assert.strictEqual(connectionsInspect?.workspaceValue, undefined);
    assert.ok(currentIdInspect?.globalValue);
    assert.strictEqual(currentIdInspect?.workspaceValue, undefined);
  });
});
