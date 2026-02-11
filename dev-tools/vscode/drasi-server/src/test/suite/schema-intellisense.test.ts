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
  test('drasi file detected by apiVersion', async () => {
    const workspaceRoot = process.env.TEST_WORKSPACE ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      assert.fail('No workspace root');
    }

    const schemaFile = getMarkedSchemaFile(workspaceRoot);

    if (!fs.existsSync(schemaFile)) {
      fs.mkdirSync(path.dirname(schemaFile), { recursive: true });
    }
    fs.writeFileSync(schemaFile, 'apiVersion: drasi.io/v1\nsources:\n  - kind: mock\n    id: demo\n');

    const doc = await vscode.workspace.openTextDocument(schemaFile);
    await vscode.window.showTextDocument(doc);

    await vscode.extensions.getExtension('DrasiProject.drasi-server')?.activate();
    await delay(500);

    // File with apiVersion: drasi.io/v1 should be treated as a drasi config
    const text = doc.getText();
    assert.ok(/^apiVersion:\s*drasi\.io\/v1\s*$/m.test(text), 'File should contain apiVersion: drasi.io/v1');
  });

  test('provides kind completions for sources', async () => {
    const workspaceRoot = process.env.TEST_WORKSPACE ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      assert.fail('No workspace root');
    }

    const schemaFile = getMarkedSchemaFile(workspaceRoot);

    fs.mkdirSync(path.dirname(schemaFile), { recursive: true });
    fs.writeFileSync(schemaFile, 'apiVersion: drasi.io/v1\nsources:\n  - kind: \n    id: demo\n');

    const doc = await vscode.workspace.openTextDocument(schemaFile);
    await vscode.window.showTextDocument(doc);

    await vscode.extensions.getExtension('redhat.vscode-yaml')?.activate();
    await vscode.extensions.getExtension('DrasiProject.drasi-server')?.activate();
    await delay(1000);

    // Verify schema was cached in global storage
    const ext = vscode.extensions.getExtension('DrasiProject.drasi-server');
    const storageUri = ext?.extensionUri;
    assert.ok(storageUri, 'Extension URI should exist');

    // Check that the schema has SourceConfig with kind enum
    const schemaConfig = vscode.workspace.getConfiguration('yaml');
    const schemaMappings = schemaConfig.get<Record<string, string[]>>('schemas') ?? {};
    const schemaKey = Object.keys(schemaMappings).find((key) => key.includes('drasi-schema'));
    if (schemaKey) {
      const schemaPath = schemaKey.replace(/^vscode-userdata:/, '').replace(/^file:/, '');
      if (fs.existsSync(schemaPath)) {
        const schema = JSON.parse(fs.readFileSync(schemaPath, 'utf8'));
        const sourceConfig = schema.definitions?.SourceConfig;
        const kindEnum = sourceConfig?.allOf?.[0]?.properties?.kind?.enum;
        assert.ok(Array.isArray(kindEnum), 'SourceConfig kind enum missing');
        assert.ok(kindEnum.includes('mock'), 'Expected mock source kind enum');
      }
    }
  });

  test('reports validation errors for invalid config', async () => {
    const workspaceRoot = process.env.TEST_WORKSPACE ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      assert.fail('No workspace root');
    }

    const schemaFile = getMarkedSchemaFile(workspaceRoot);

    fs.mkdirSync(path.dirname(schemaFile), { recursive: true });
    fs.writeFileSync(schemaFile, 'apiVersion: drasi.io/v1\nport: "${SERVER_PORT:-8080}"\n');

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

    fs.mkdirSync(path.dirname(schemaFile), { recursive: true });
    fs.writeFileSync(schemaFile, 'apiVersion: drasi.io/v1\nsources:\\n  - kind: mock\\n    id: demo\\n');

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

  test('bootstrap provider inherits source fields without required errors', async () => {
    const workspaceRoot = process.env.TEST_WORKSPACE ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      assert.fail('No workspace root');
    }

    const schemaFile = getMarkedSchemaFile(workspaceRoot);

    fs.mkdirSync(path.dirname(schemaFile), { recursive: true });
    fs.writeFileSync(
      schemaFile,
      [
        'apiVersion: drasi.io/v1',
        'sources:',
        '  - kind: postgres',
        '    id: demo',
        '    host: localhost',
        '    port: 5432',
        '    database: demo_db',
        '    user: demo_user',
        '    password: demo_pass',
        '    bootstrapProvider:',
        '      kind: postgres',
        '',
      ].join('\n'),
    );

    const doc = await vscode.workspace.openTextDocument(schemaFile);
    await vscode.window.showTextDocument(doc);

    await vscode.extensions.getExtension('DrasiProject.drasi-server')?.activate();
    await delay(1000);

    const diagnostics = vscode.languages.getDiagnostics(doc.uri);
    assert.ok(
      diagnostics.every((diag) => !diag.message.toLowerCase().includes('missing required property')),
      `Expected no missing required property diagnostics, got: ${diagnostics.map((diag) => diag.message).join(', ')}`
    );
  });
});
