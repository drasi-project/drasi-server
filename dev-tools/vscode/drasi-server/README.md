# Drasi Server - Visual Studio Code Extension

The **Drasi Server** extension provides tools for managing and debugging Drasi Server resources directly inside Visual Studio Code. It offers features like a workspace explorer for YAML files, a live resource explorer, code actions for applying and debugging resources, and real-time query debugging.

## Features

- **Workspace Explorer**: Browse and manage YAML files containing Drasi resources (Queries, Sources, Reactions)
- **Drasi Explorer**: View and interact with live resources running in drasi-server
- **Saved Servers**: Maintain a list of Drasi Server connections and switch between them
- **CodeLens Support**: Apply resources or launch the server directly from YAML files using inline actions
- **Launch Server**: Start a drasi-server instance from a config file with a single click
- **Query Debugger**: Debug queries with real-time results in a webview
- **Event & Log Streaming**: Stream events and logs from sources, queries, and reactions in real time
- **YAML Intellisense**: Schemas fetched from the server's OpenAPI endpoint for autocompletion and validation
- **YAML Generators**: Scaffold Source, Query, and Reaction YAML from the schema via guided prompts
- **Auto-Detection**: YAML files with `apiVersion: drasi.io/v1` are automatically recognized as Drasi config files

## Requirements

- Drasi Server running (default: `http://localhost:8080`)
- Red Hat YAML extension (bundled — installed automatically)

## Configuration

| Setting | Description |
|---------|-------------|
| `drasiServer.url` | Base URL for the Drasi Server API (default: `http://localhost:8080`) |
| `drasiServer.instanceId` | Optional instance ID to use; if empty, the first instance is selected |
| `drasiServer.connections` | Saved server connections |
| `drasiServer.currentConnectionId` | Active connection ID |
| `drasiServer.binaryPath` | Path to the `drasi-server` binary (used by the Launch Server command) |

## Getting Started

### Add a Server

Use the **Drasi** view in the activity bar:

1. Right-click an existing server entry
2. Select **Add server**
3. Provide the server URL and a friendly name

To edit the active server URL, choose **Edit server URL**.

### Launch a Server

Open a Drasi config YAML file (one with `apiVersion: drasi.io/v1`). A **▶ Launch Server** code lens appears at the top. Click it to:

1. Select the `drasi-server` binary (first time only — saved to `drasiServer.binaryPath`)
2. Confirm or override the port number
3. The server launches in a VS Code terminal

You can also set the binary path ahead of time via the **Drasi Server: Configure Server Binary** command.

### Apply Resources

Individual sources, queries, and reactions in a Drasi YAML file show an **Apply** code lens. Click it to upsert the resource to the connected server.

### Create Resources

Right-click in a YAML editor and choose **Create Source YAML**, **Create Query YAML**, or **Create Reaction YAML** to scaffold a new resource from the server's schema.

## Development

```bash
cd dev-tools/vscode/drasi-server
npm install
npm run compile
```

Use the **Run Drasi Server Extension** launch configuration to start a development host.

### Testing

```bash
npm test
```

## License

Apache 2.0
