import * as path from 'path';
import { runTests, runVSCodeCommand } from '@vscode/test-electron';

async function main() {
  try {
    const extensionDevelopmentPath = path.resolve(__dirname, '../../');
    const extensionTestsPath = path.resolve(__dirname, './suite/index');
    const testWorkspace = path.resolve(__dirname, '../../test-fixtures/fixture1');
    const userDataDir = path.resolve(extensionDevelopmentPath, '.vscode-test/user-data');
    const extensionsDir = path.resolve(extensionDevelopmentPath, '.vscode-test/extensions');

    await runVSCodeCommand(
      [
        '--install-extension',
        'redhat.vscode-yaml',
        `--extensions-dir=${extensionsDir}`,
        `--user-data-dir=${userDataDir}`,
      ],
      {
        reuseMachineInstall: false,
      }
    );

    await runTests({
      extensionDevelopmentPath,
      extensionTestsPath,
      launchArgs: [
        testWorkspace,
        `--user-data-dir=${userDataDir}`,
        `--extensions-dir=${extensionsDir}`,
      ],
      extensionTestsEnv: {
        TEST_WORKSPACE: testWorkspace,
      },
      reuseMachineInstall: false,
    });
  } catch (error) {
    console.error('Failed to run extension tests');
    console.error(error);
    process.exit(1);
  }
}

void main();
