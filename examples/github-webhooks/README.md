# GitHub Webhooks with Drasi Server

This example demonstrates how to use Drasi Server's HTTP source in **webhook mode** to receive and process GitHub webhook events in real-time. It showcases a practical integration scenario where continuous queries react to repository activity — pushes, pull requests, and issues.

## What You'll Build

A real-time GitHub activity monitoring system that:
- Receives webhook events from GitHub via HTTP
- Verifies payload signatures using HMAC-SHA256
- Transforms webhook payloads into graph nodes using configurable mappings
- Runs continuous Cypher queries over GitHub activity
- Streams results to the browser via SSE and logs to the console

## Architecture

```
┌─────────────────┐     ┌──────────────────────────────────────┐     ┌──────────────────┐
│  GitHub / Curl  │────▶│         Drasi Server                 │────▶│  Browser (SSE)   │
│  (Webhooks)     │POST │                                      │ SSE │  viewer/index    │
│                 │     │  Source:                             │     │     .html        │
│  Events:        │     │  - github-webhooks (HTTP webhook)    │     └──────────────────┘
│  • push         │     │                                      │
│  • pull_request │     │  Queries:                            │     ┌──────────────────┐
│  • issues       │     │  - all-activity                      │────▶│  Console (Log)   │
│                 │     │  - recent-pushes                     │ Log │  reaction        │
└─────────────────┘     │  - open-pull-requests                │     └──────────────────┘
                        │  - issues-opened                     │
                        │                                      │     ┌──────────────────┐
                        │  Reactions:                           │     │  REST API        │
                        │  - log-github-activity               │────▶│  port 8080       │
                        │  - sse-github-events                 │     └──────────────────┘
                        └──────────────────────────────────────┘
```

### Data Flow

1. **GitHub sends a webhook** (or you simulate one with the provided scripts)
2. **HTTP source verifies** the HMAC-SHA256 signature against your secret
3. **Webhook mappings** route different event types (push, PR, issue) based on the `X-GitHub-Event` header
4. **Templates** extract fields from the JSON payload and create graph nodes with appropriate labels and properties
5. **Continuous queries** filter and project the data
6. **Reactions** deliver results to the console log and SSE stream

## Prerequisites

- **Rust/Cargo**: Required to build Drasi Server — [Install Rust](https://rustup.rs/)
- **curl**: For running simulation scripts
- **openssl**: For HMAC signature computation in simulation scripts

## Quick Start

### 1. Start the Server

```bash
cd scripts
./start-server.sh
```

This builds and starts Drasi Server with the webhook configuration. The server will:
- Listen for API requests on port **8080**
- Listen for GitHub webhooks on port **9000** at `/github/events`
- Stream SSE events on port **8081** at `/events`

### 2. Simulate GitHub Events

In a separate terminal, send simulated webhook events:

```bash
# Simulate a push event
./scripts/simulate-push.sh

# Simulate a pull request being opened
./scripts/simulate-pr.sh

# Simulate an issue being opened
./scripts/simulate-issue.sh
```

Each script sends a realistic GitHub webhook payload with a valid HMAC-SHA256 signature.

### 3. View Results

Check query results via the REST API:

```bash
./scripts/view-results.sh
```

Or stream events in real-time:

```bash
./scripts/stream-events.sh
```

### 4. Open the Live Viewer

Open `viewer/index.html` in your browser to see a live dashboard of GitHub events streaming via SSE.

## Connecting to a Real GitHub Repository

To receive real GitHub webhooks you need a public URL that GitHub can reach. This section walks through using [Microsoft Dev Tunnels](https://learn.microsoft.com/en-us/azure/developer/dev-tunnels/overview) to expose your local webhook endpoint.

### 1. Install the Dev Tunnels CLI

If you don't already have the `devtunnel` CLI:

```bash
# macOS
brew install --cask devtunnel

# Linux (Debian/Ubuntu)
curl -sL https://aka.ms/DevTunnelCliInstall | bash

# Windows (winget)
winget install Microsoft.devtunnel
```

Verify the installation:

```bash
devtunnel --version
```

### 2. Log in

Authenticate with your Microsoft or GitHub account:

```bash
# Log in with GitHub (recommended — works with any GitHub account)
devtunnel user login --github

# Or with a Microsoft account
devtunnel user login
```

### 3. Create and start the tunnel

Expose the webhook port (9000 by default) with anonymous access so GitHub can reach it without additional auth headers:

```bash
devtunnel host -p 9000 --allow-anonymous
```

The CLI will output a public URL like:

```
Connect via browser: https://abc123-9000.usw2.devtunnels.ms
```

> **Tip:** To run the tunnel in the background and give it a persistent name:
> ```bash
> devtunnel create --name github-webhooks
> devtunnel port create github-webhooks --port-number 9000
> devtunnel access create github-webhooks --port-number 9000 --anonymous
> devtunnel host github-webhooks
> ```

### 4. Generate a webhook secret

For real usage, generate a strong secret:

```bash
# Generate a random 32-byte hex secret
openssl rand -hex 32
```

Update your `.env` file with the generated secret:

```bash
GITHUB_WEBHOOK_SECRET=<your-generated-secret>
```

### 5. Configure the GitHub Webhook

1. Go to your repository on GitHub → **Settings** → **Webhooks** → **Add webhook**
2. Set:
   - **Payload URL**: `https://<your-tunnel-id>-9000.usw2.devtunnels.ms/github/events`
     (use the URL from step 3, appending `/github/events`)
   - **Content type**: `application/json`
   - **Secret**: The secret you generated in step 4
   - **Events**: Select "Send me everything" or choose specific events:
     - ✅ Pushes
     - ✅ Pull requests
     - ✅ Issues
3. Click **Add webhook**

GitHub will immediately send a `ping` event. You should see a green checkmark (✓) next to the webhook once Drasi Server responds with 200 OK.

### 6. Start Drasi Server

Make sure your `.env` has the correct secret, then start the server:

```bash
cd scripts
./start-server.sh
```

### 7. Verify end-to-end

Push a commit or open a PR/issue on your repository. You should see:
- Log output in the server console
- Events in the SSE stream (`./scripts/stream-events.sh`)
- Live updates in `viewer/index.html`

### Troubleshooting Dev Tunnels

| Issue | Solution |
|-------|----------|
| `devtunnel: command not found` | Ensure the CLI is installed and on your PATH |
| GitHub shows "delivery failed" | Verify the tunnel is running and the port matches `WEBHOOK_PORT` in `.env` |
| 401 from Drasi | Ensure `GITHUB_WEBHOOK_SECRET` in `.env` matches the secret in GitHub webhook settings |
| Tunnel URL changed | Dev tunnels get a new URL each time unless you use a named tunnel (see persistent tunnel tip above) |

### Alternative: Using ngrok

If you prefer [ngrok](https://ngrok.com/):

```bash
ngrok http 9000
```

Then use the ngrok URL (e.g., `https://abc123.ngrok-free.app/github/events`) as the Payload URL in GitHub.

## Configuration Deep Dive

### Webhook Source Configuration

The HTTP source uses **webhook mode** (enabled by the `webhooks` section) which provides:

- **Route matching**: Path patterns with optional path parameters
- **Authentication**: HMAC-SHA256 signature verification matching GitHub's format
- **Conditional mappings**: Route different event types based on headers
- **Template engine**: Handlebars templates to extract fields from payloads

### Key Configuration Patterns

**Routing by event type** using the `when` condition:
```yaml
- when:
    header: X-GitHub-Event
    equals: push
```

**Extracting nested fields** using Handlebars dot notation:
```yaml
template:
  id: "push-{{payload.head_commit.id}}"
  properties:
    author: "{{payload.head_commit.author.username}}"
```

**HMAC signature verification** matching GitHub's format:
```yaml
auth:
  signature:
    type: hmac-sha256
    secretEnv: GITHUB_WEBHOOK_SECRET
    header: X-Hub-Signature-256
    prefix: "sha256="
    encoding: hex
```

### Queries

| Query | Description |
|-------|-------------|
| `recent-pushes` | Push events with commit details |
| `open-pull-requests` | Pull requests in "open" state |
| `issues-opened` | Newly opened issues |

## Extending This Example

### Add more event types

GitHub supports many event types. Add new mappings for events like:
- `star` — Track repository stars
- `workflow_run` — Track CI/CD activity
- `release` — Track new releases

Example mapping for star events:
```yaml
- when:
    header: X-GitHub-Event
    equals: star
  operation: insert
  elementType: node
  template:
    id: "star-{{payload.sender.id}}"
    labels: [StarEvent]
    properties:
      repo: "{{payload.repository.full_name}}"
      action: "{{payload.action}}"
      user: "{{payload.sender.login}}"
```

### Add an HTTP reaction

Forward processed events to another service (e.g., Slack, Discord):

```yaml
reactions:
  - kind: http
    id: slack-notifications
    queries:
      - open-pull-requests
    autoStart: true
    baseUrl: "https://hooks.slack.com/services/YOUR/WEBHOOK/URL"
    routes:
      open-pull-requests:
        added:
          url: ""
          method: POST
          body: '{"text": "New PR #{{after.PRNumber}}: {{after.Title}} by {{after.Author}}"}'
```

## Troubleshooting

### Webhook returns 401 Unauthorized

The HMAC signature doesn't match. Ensure:
- `GITHUB_WEBHOOK_SECRET` in `.env` matches the secret configured in GitHub
- The simulation scripts are using the same secret

### Events aren't appearing in queries

Check the server logs for mapping errors. Common issues:
- Payload field paths don't match (e.g., GitHub changed their webhook schema)
- The `X-GitHub-Event` header value doesn't match the `when.equals` condition

### SSE connection drops

The SSE reaction has a 30-second heartbeat. If your browser disconnects, it should reconnect automatically (the viewer page handles this).

## Related Examples

- [HTTP Webhook Receiver](../configs/02-sources/http-webhook-receiver.yaml) — Simpler HTTP source without webhook mode
- [HTTP Webhook Sender](../configs/03-reactions/http-webhook-sender.yaml) — HTTP reaction for outbound webhooks
- [Getting Started](../getting-started/) — PostgreSQL CDC tutorial
