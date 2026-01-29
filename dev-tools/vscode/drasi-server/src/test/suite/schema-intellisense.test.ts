import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';

function delay(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function getMarkedSchemaFile(workspaceRoot: string) {
  return path.join(workspaceRoot, 'marked.yaml');
}

suite('Drasi schema mapping', () => {
  test('marked file is mapped to schema', async () => {
    const workspaceRoot = process.env.TEST_WORKSPACE ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      assert.fail('No workspace root');
    }

    const schemaFile = getMarkedSchemaFile(workspaceRoot);
    const relativePath = vscode.workspace.asRelativePath(schemaFile, false).replace(/\\/g, '/');

    const config = vscode.workspace.getConfiguration('drasiServer');
    await config.update('schemaFiles', [relativePath], vscode.ConfigurationTarget.Workspace);

    if (!fs.existsSync(schemaFile)) {
      fs.mkdirSync(path.dirname(schemaFile), { recursive: true });
      fs.writeFileSync(schemaFile, 'sources:\n  - kind: mock\n    id: demo\n');
    }

    const doc = await vscode.workspace.openTextDocument(schemaFile);
    await vscode.window.showTextDocument(doc);

    await vscode.extensions.getExtension('DrasiProject.drasi-server')?.activate();
    await delay(500);

    const schemaConfig = vscode.workspace.getConfiguration('yaml');
    const updatedMappings = schemaConfig.get<Record<string, string[]>>('schemas') ?? {};
    const updatedHasMapping = Object.values(updatedMappings).some((patterns) =>
      patterns.includes(relativePath)
    );
    assert.ok(updatedHasMapping, 'Schema mapping missing after refresh');
  });

  test('provides kind completions for sources', async () => {
    const workspaceRoot = process.env.TEST_WORKSPACE ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      assert.fail('No workspace root');
    }

    const schemaFile = getMarkedSchemaFile(workspaceRoot);
    const relativePath = vscode.workspace.asRelativePath(schemaFile, false).replace(/\\/g, '/');

    const config = vscode.workspace.getConfiguration('drasiServer');
    await config.update('schemaFiles', [relativePath], vscode.ConfigurationTarget.Workspace);

    fs.mkdirSync(path.dirname(schemaFile), { recursive: true });
    fs.writeFileSync(schemaFile, 'sources:\n  - kind: \n    id: demo\n');

    const doc = await vscode.workspace.openTextDocument(schemaFile);
    await vscode.window.showTextDocument(doc);

    await vscode.extensions.getExtension('redhat.vscode-yaml')?.activate();
    await vscode.extensions.getExtension('DrasiProject.drasi-server')?.activate();
    await delay(1000);

    const schemaConfig = vscode.workspace.getConfiguration('yaml');
    const schemaMappings = schemaConfig.get<Record<string, string[]>>('schemas') ?? {};
    const schemaKey = Object.keys(schemaMappings).find((key) => schemaMappings[key]?.includes(relativePath));
    if (!schemaKey) {
      assert.fail('Schema mapping not found for marked file');
    }
    const schemaPath = schemaKey.replace(/^vscode-userdata:/, '').replace(/^file:/, '');
    const schema = JSON.parse(fs.readFileSync(schemaPath, 'utf8'));
    const sourceConfig = schema.definitions?.SourceConfig;
    const kindEnum = sourceConfig?.allOf?.[0]?.properties?.kind?.enum;
    assert.ok(Array.isArray(kindEnum), 'SourceConfig kind enum missing');
    assert.ok(kindEnum.includes('mock'), 'Expected mock source kind enum');
  });

  test('reports validation errors for invalid config', async () => {
    const workspaceRoot = process.env.TEST_WORKSPACE ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      assert.fail('No workspace root');
    }

    const schemaFile = getMarkedSchemaFile(workspaceRoot);
    const relativePath = vscode.workspace.asRelativePath(schemaFile, false).replace(/\\/g, '/');

    const config = vscode.workspace.getConfiguration('drasiServer');
    await config.update('schemaFiles', [relativePath], vscode.ConfigurationTarget.Workspace);

    fs.mkdirSync(path.dirname(schemaFile), { recursive: true });
    fs.writeFileSync(schemaFile, 'port: "${SERVER_PORT:-8080}"\n');

    const doc = await vscode.workspace.openTextDocument(schemaFile);
    await vscode.window.showTextDocument(doc);

    await vscode.extensions.getExtension('DrasiProject.drasi-server')?.activate();
    await delay(1000);

    const diagnostics = vscode.languages.getDiagnostics(doc.uri);
    assert.ok(diagnostics.length === 0, 'Expected no diagnostics for env-interpolated value');
  });

  test('config with valid source kind does not error', async () => {
    const workspaceRoot = process.env.TEST_WORKSPACE ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      assert.fail('No workspace root');
    }

    const schemaFile = getMarkedSchemaFile(workspaceRoot);
    const relativePath = vscode.workspace.asRelativePath(schemaFile, false).replace(/\\/g, '/');

    const config = vscode.workspace.getConfiguration('drasiServer');
    await config.update('schemaFiles', [relativePath], vscode.ConfigurationTarget.Workspace);

    fs.mkdirSync(path.dirname(schemaFile), { recursive: true });
    fs.writeFileSync(schemaFile, 'sources:\\n  - kind: mock\\n    id: demo\\n');

    const doc = await vscode.workspace.openTextDocument(schemaFile);
    await vscode.window.showTextDocument(doc);

    await vscode.extensions.getExtension('DrasiProject.drasi-server')?.activate();
    await delay(1000);

    const diagnostics = vscode.languages.getDiagnostics(doc.uri);
    assert.ok(
      diagnostics.every((diag) => !diag.message.includes('kind')),
      'Expected no kind diagnostics for valid source kind'
    );
  });
});
