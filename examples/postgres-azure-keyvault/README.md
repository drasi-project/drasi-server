# PostgreSQL with Azure Key Vault Example

This example demonstrates how to use the **Azure Key Vault secret store plugin**
to resolve database credentials from Azure Key Vault instead of hardcoding them
in your configuration file.

```yaml
password:
  kind: Secret
  name: DB-PASSWORD        # resolved from Azure Key Vault at runtime
```

## Prerequisites

- Docker (for the PostgreSQL container)
- A built Drasi Server binary (`cargo build`)
- The **Azure Key Vault secret store plugin** (`libdrasi_secret_store_azure_keyvault.so`)
  in the server's `plugins/` directory. Build it from drasi-core:

  ```bash
  cd ../drasi-core
  cargo build --release -p drasi-secret-store-azure-keyvault --features dynamic-plugin
  cp target/release/libdrasi_secret_store_azure_keyvault.so ../drasi-server/target/release/plugins/
  ```

- An Azure Key Vault with a secret named `DB-PASSWORD` containing the PostgreSQL
  password. Key Vault secret names use hyphens — underscores are not allowed.

## Azure Setup

### 1. Create a Key Vault (if you don't have one)

```bash
az group create --name drasi-demo-rg --location eastus
az keyvault create --name drasi-demo-kv --resource-group drasi-demo-rg --location eastus
```

### 2. Store the PostgreSQL password

```bash
az keyvault secret set --vault-name drasi-demo-kv --name DB-PASSWORD --value "Drasi@Pass123"
```

### 3. Grant access

The identity running Drasi Server needs the **Key Vault Secrets User** role:

```bash
# For your own user (local development with `az login`)
az role assignment create \
  --role "Key Vault Secrets User" \
  --assignee $(az ad signed-in-user show --query id -o tsv) \
  --scope $(az keyvault show --name drasi-demo-kv --query id -o tsv)
```

### 4. Authenticate locally

```bash
az login
```

The `developer_tools` auth method (default in the example config) uses your
`az login` session, VS Code Azure account, or IntelliJ Azure toolkit
credentials.

## Quick Start

### 1. Edit the config

Open `server-config.yaml` and replace `YOUR-VAULT-NAME` with your Key Vault
name:

```yaml
secretStore:
  kind: azure-keyvault
  vaultUrl: https://drasi-demo-kv.vault.azure.net/
  authMethod: developer_tools
```

### 2. Start PostgreSQL

Use the same Docker setup from the `postgres-secrets` example:

```bash
./examples/postgres-secrets/docker-start-postgres.sh
```

### 3. Run Drasi Server

```bash
cargo run -- --skip-verification --config examples/postgres-azure-keyvault/server-config.yaml
```

### 4. Observe

The server will:
1. Load the Azure Key Vault secret store plugin
2. Authenticate to Azure using your `az login` session
3. Create the PostgreSQL source, resolving `DB-PASSWORD` from Key Vault
4. Start the `high-temp` query and `log-temps` reaction

### 5. Test live changes

```bash
docker exec -it drasi-postgres-secrets psql -U postgres -d drasi_demo
UPDATE sensors SET temperature = 90.0 WHERE name = 'sensor-1';
```

Change events will appear in stdout via the log reaction.

## Authentication Methods

The config file includes 5 auth method options. Uncomment the one that fits your
environment:

| Method | `authMethod` | When to use |
| --- | --- | --- |
| Developer tools | `developer_tools` | Local dev after `az login` |
| System managed identity | `managed_identity` | Azure VMs, App Service, ACI |
| User-assigned managed identity | `managed_identity_user_assigned` | Shared identity across resources |
| Workload identity | `workload_identity` | AKS with federated identity |
| Client secret | `client_secret` | Service principal (CI/CD, non-Azure hosts) |

## Files

| File | Purpose |
| --- | --- |
| `server-config.yaml` | Drasi Server config with Azure Key Vault secret store |
| `README.md` | This file |

> **Note:** This example reuses the PostgreSQL Docker container from
> `examples/postgres-secrets/`. Run `docker-start-postgres.sh` from that
> directory first.

## How It Works

```
server-config.yaml            Azure Key Vault
┌──────────────────┐          ┌──────────────────────┐
│ secretStore:     │          │ drasi-demo-kv         │
│   kind:          │   REST   │                      │
│    azure-keyvault│─────────▶│ DB-PASSWORD = ...     │
│   vaultUrl: ...  │   API    │                      │
│                  │          └──────────────────────┘
│ sources:         │
│   password:      │
│     kind: Secret │── resolved at runtime ──▶ "Drasi@Pass123"
│     name:        │
│       DB-PASSWORD│
└──────────────────┘
```

## Cleanup

```bash
docker rm -f drasi-postgres-secrets
# Optionally remove Azure resources:
# az group delete --name drasi-demo-rg --yes
```
