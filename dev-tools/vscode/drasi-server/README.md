# Drasi Server - Visual Studio Code Extension

The **Drasi Server** extension provides tools for managing and debugging Drasi Server resources directly inside Visual Studio Code. It is inspired by the original Drasi Platform extension, but uses the standalone drasi-server REST API and dynamically loads schemas at runtime.

## Features

- **Workspace Explorer**: Browse and manage YAML files containing Drasi resources (Queries, Sources, Reactions)
- **Drasi Explorer**: View and interact with live resources running in drasi-server
- **Saved Servers**: Maintain a list of Drasi Server connections and switch between them
- **CodeLens Support**: Apply or debug resources directly from YAML files using inline actions
- **Query Debugger**: Debug queries with real-time results in a webview
- **Query Watcher**: Watch running queries for live result updates
- **Runtime YAML Intellisense**: Schemas fetched from `/api/v1/openapi.json` for autocompletion and validation

## Requirements

- Drasi Server running (default: `http://localhost:8080`)
- Red Hat YAML extension installed

## Configuration

- `drasiServer.connections` - Saved server connections
- `drasiServer.currentConnectionId` - Active connection ID
- `drasiServer.url` / `drasiServer.instanceId` - Legacy single-connection fields (used to seed the first connection)
- `drasiServer.schemaFiles` - Workspace-relative YAML files to treat as Drasi configs

## Add a Server

Use the **Drasi** view in the activity bar:

1. Right-click an existing server entry
2. Select **Add server**
3. Provide the server URL and a friendly name

To edit the active server URL, choose **Edit server URL**.

## Mark a YAML File as Drasi

If a YAML file does not match the default naming patterns, you can mark it manually:

1. Open the YAML file
2. Right-click in the editor
3. Toggle **Drasi Config**

This stores the file in workspace settings and enables schema validation.
Toggle it again to unmark.

## Development

```bash
cd dev-tools/vscode/drasi-server
npm install
npm run compile
```

Use the **Run Drasi Server Extension** launch configuration to start a development host.

## License

Apache 2.0
