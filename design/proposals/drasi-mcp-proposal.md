# Proposal: MCP Mode for Drasi Server

## Overview

This proposal adds a new **MCP (Model Context Protocol) mode** to `drasi-server`, allowing it to run as a stdio-based MCP server instead of (or alongside) its HTTP API. When launched in MCP mode, Drasi communicates over stdin/stdout using the JSON-RPC 2.0 protocol, making it directly usable as a tool by AI agents such as Claude, GitHub Copilot, Cursor, and any MCP-compatible client.

The proposal also defines a set of **agent skills** — MCP prompts that encode Drasi domain expertise into reusable, higher-order workflows that any AI agent can invoke.

## Motivation

### The Rise of Always-On AI Agents

The AI agent landscape is shifting from request-response interactions to **persistent, always-running autonomous agents**. This trend is accelerating across the industry:

- **OpenClaw** is an open-source framework purpose-built for 24/7 autonomous agents that run continuously on self-hosted infrastructure. OpenClaw agents maintain long-term memory across sessions, execute scheduled and triggered tasks while the user is offline, and can operate as coordinated swarms — turning AI from a tool you invoke into a digital workforce that acts on your behalf around the clock.

- **Kimi K2.5** (Moonshot AI) introduced Agent Swarm — the ability to orchestrate up to 100 persistent sub-agents in parallel, each pursuing its own sub-tasks autonomously, coordinating with others, and running until a goal or condition is met. With 256k+ token context windows, Kimi agents maintain state across days or weeks, enabling long-running workflows like continuous monitoring, research, and compliance automation.

- **Claude** (Anthropic) has evolved from a chat assistant to a persistent agentic platform. Claude Code's "Tasks" feature stores durable, stateful work items that survive across reboots and sessions, enabling multi-day project-spanning workflows. Anthropic's engineering guidance on "effective harnesses for long-running agents" describes initializer agents that set up context for long-lived projects, with workers making incremental, documented progress that can be paused, resumed, and handed off.

These are not chatbots. They are **autonomous processes** that run continuously, monitor their environment, and act when conditions change.

### The Missing Piece: Efficient Data Watching

Always-on agents have a fundamental problem: **how do they know when something changes?** The naive approach is polling — periodically querying databases, APIs, and systems to check for changes. This is wasteful, slow, and scales poorly. An agent monitoring 50 data conditions across 10 systems would need hundreds of polling loops, most returning "nothing changed."

**Drasi solves this.** Instead of the agent polling, Drasi continuously evaluates declarative queries against data sources and notifies the agent only when results change. The agent goes from "check everything every 30 seconds" to "get told exactly what changed, the moment it changes."

This is the core value proposition for always-on agents:

| Without Drasi | With Drasi |
|---|---|
| Agent polls databases on a loop | Drasi pushes changes to the agent |
| Agent wastes tokens re-querying unchanged data | Agent only processes actual changes |
| Agent must implement polling, diffing, and deduplication | Drasi handles change detection declaratively |
| Latency = polling interval (seconds to minutes) | Latency = real-time (sub-second for CDC sources) |
| Agent must stay awake to watch | Drasi watches; agent can sleep until notified |
| Scaling to N conditions = N polling loops | Scaling to N conditions = N Cypher queries (evaluated efficiently by Drasi's engine) |
| Cross-source correlation requires agent-side joins | Drasi joins across sources in the query layer |

For a 24/7 agent managing infrastructure, monitoring business metrics, or guarding data quality, Drasi acts as a **persistent sensory system** — the agent's eyes and ears on the data, running continuously and cheaply, waking the agent's reasoning only when there's something to reason about.

### Why MCP?

Today, an AI agent that wants to use Drasi must:
1. Know that Drasi exists and where its HTTP API is running
2. Understand the REST API surface (OpenAPI spec)
3. Write custom HTTP client code to interact with it
4. Know Cypher syntax, Drasi-specific functions, and the data schema
5. Orchestrate multi-step workflows (create source → create query → create reaction → monitor results)

MCP eliminates steps 1–3 entirely. The AI client discovers Drasi's tools automatically via the MCP protocol. Agent skills (MCP prompts) address steps 4–5 by encoding Drasi expertise into guided workflows.

### Why stdio, not HTTP?

- **Zero configuration**: The AI client launches `drasi-server mcp` as a subprocess. No ports, no URLs, no firewalls.
- **Native integration**: Claude Desktop, VS Code Copilot, Cursor, and other MCP clients expect stdio-based servers. This is the standard local integration pattern.
- **Security**: No network surface. Communication is process-local.
- **Simplicity**: The MCP spec defines stdio as newline-delimited JSON-RPC over stdin/stdout. No HTTP server needed for MCP mode.

## Design

### CLI Interface

A new `Mcp` subcommand is added to the existing CLI:

```
drasi-server mcp [--config <path>]
```

- Reads config from the specified YAML file (same format as `run` mode)
- Communicates via stdin/stdout using MCP JSON-RPC protocol
- Logs to stderr (per MCP spec — stdout is reserved for protocol messages)
- The HTTP API is **not started** in MCP mode (stdout must be clean)

### Architecture

```
┌──────────────────────┐     stdin/stdout      ┌──────────────────────┐
│   AI Agent (Claude,  │◄────JSON-RPC 2.0─────►│   drasi-server mcp   │
│   Copilot, Cursor)   │                        │                      │
│                      │                        │  ┌────────────────┐  │
│  MCP Client          │                        │  │  MCP Handler   │  │
│  - discovers tools   │                        │  │  (rmcp SDK)    │  │
│  - calls tools       │                        │  │                │  │
│  - reads resources   │                        │  │  Tools ────────┤  │
│  - invokes prompts   │                        │  │  Resources ────┤  │
│                      │                        │  │  Prompts ──────┤  │
└──────────────────────┘                        │  └───────┬────────┘  │
                                                │          │           │
                                                │  ┌───────▼────────┐  │
                                                │  │  DrasiLib Core │  │
                                                │  │  (same engine) │  │
                                                │  └────────────────┘  │
                                                └──────────────────────┘
                                                         │
                                              ┌──────────┴──────────┐
                                              ▼                     ▼
                                       ┌──────────┐          ┌──────────┐
                                       │ Postgres │          │  HTTP    │
                                       │   CDC    │          │ Webhooks │
                                       └──────────┘          └──────────┘
```

The key insight: `DrasiLib` (the core engine) is already a library. In `run` mode, it's wrapped by an Axum HTTP server. In `mcp` mode, it's wrapped by an MCP handler instead. Both modes use the same underlying engine, config format, and plugin system.

### Rust Implementation

Use the official `rmcp` crate (the Rust MCP SDK) with stdio transport:

```toml
# Cargo.toml addition
[dependencies]
rmcp = { version = "0.3", features = ["server", "transport-io"] }
```

The MCP handler struct holds a reference to the DrasiLib core and exposes tools, resources, and prompts via `rmcp` macros:

```rust
use rmcp::{tool, tool_router, tool_handler, ServerHandler, ServiceExt};
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::transport::stdio;

#[derive(Clone)]
pub struct DrasiMcpServer {
    core: Arc<DrasiCore>,       // shared DrasiLib engine
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl DrasiMcpServer {
    #[tool(description = "List all configured data sources")]
    async fn list_sources(&self) -> String { /* ... */ }

    #[tool(description = "Create a new continuous Cypher query")]
    async fn create_query(&self, id: String, query: String, sources: Vec<String>) -> String { /* ... */ }

    // ... more tools
}

#[tool_handler]
impl ServerHandler for DrasiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Drasi is a real-time change detection engine...".into()),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
            ..Default::default()
        }
    }
}
```

Entry point in `main.rs`:

```rust
Some(Commands::Mcp { config }) => run_mcp_server(config).await,
```

```rust
async fn run_mcp_server(config_path: PathBuf) -> Result<()> {
    // Redirect all logging to stderr (stdout is MCP protocol)
    // Load config, init DrasiLib core (same as run_server)
    let core = init_drasi_core(&config_path).await?;
    let mcp = DrasiMcpServer::new(core);
    let service = mcp.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

## MCP Tools

These are the executable functions the AI agent can call:

### Source Management

| Tool | Parameters | Description |
|---|---|---|
| `list_sources` | `instanceId?` | List all configured data sources with their status |
| `get_source` | `instanceId?, sourceId` | Get detailed status and config for a source |
| `create_source` | `instanceId?, config (JSON)` | Create a new data source |
| `delete_source` | `instanceId?, sourceId` | Remove a data source |
| `start_source` | `instanceId?, sourceId` | Start a stopped source |
| `stop_source` | `instanceId?, sourceId` | Stop a running source |

### Query Management

| Tool | Parameters | Description |
|---|---|---|
| `list_queries` | `instanceId?` | List all continuous queries with their status |
| `get_query` | `instanceId?, queryId` | Get query config and status |
| `create_query` | `instanceId?, id, query, queryLanguage?, sources, joins?, autoStart?` | Create a new continuous query |
| `delete_query` | `instanceId?, queryId` | Remove a query |
| `start_query` | `instanceId?, queryId` | Start a stopped query |
| `stop_query` | `instanceId?, queryId` | Stop a running query |
| `get_query_results` | `instanceId?, queryId` | Get current result snapshot |
| `validate_query` | `instanceId?, id, query, queryLanguage?, sources` | Validate a query without creating it (dry run) |

### Reaction Management

| Tool | Parameters | Description |
|---|---|---|
| `list_reactions` | `instanceId?` | List all configured reactions with their status |
| `get_reaction` | `instanceId?, reactionId` | Get reaction config and status |
| `create_reaction` | `instanceId?, config (JSON)` | Create a new reaction |
| `delete_reaction` | `instanceId?, reactionId` | Remove a reaction |
| `start_reaction` | `instanceId?, reactionId` | Start a stopped reaction |
| `stop_reaction` | `instanceId?, reactionId` | Stop a running reaction |

### Discovery & Introspection

| Tool | Parameters | Description |
|---|---|---|
| `get_schema` | `instanceId?` | Get the graph schema: all known labels, properties, and their sources (via `Source::describe_schema()`) |
| `get_query_context` | `instanceId?` | Get schema + Drasi Cypher reference + example patterns in one call |
| `list_instances` | — | List all DrasiLib instances |

### Instance Management

| Tool | Parameters | Description |
|---|---|---|
| `get_health` | — | Get server health status |

All tools that accept `instanceId?` default to the first configured instance when omitted, matching the existing REST API convenience route pattern.

## Schema Discovery Architecture

Schema discovery is powered by an optional abstraction on the `Source` trait, not by reverse-engineering config files. Each source plugin knows its own schema best and can describe it at the level of detail appropriate for its data.

### Source Trait Extension

A new default method is added to the existing `Source` trait in drasi-lib:

```rust
#[async_trait]
pub trait Source: Send + Sync {
    // ... existing required methods ...
    fn id(&self) -> &str;
    fn type_name(&self) -> &str;
    fn properties(&self) -> HashMap<String, serde_json::Value>;
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn status(&self) -> ComponentStatus;
    async fn subscribe(&self, settings: SourceSubscriptionSettings) -> Result<SubscriptionResponse>;

    /// Describe the graph schema this source provides.
    /// Returns None if the source cannot describe its schema.
    fn describe_schema(&self) -> Option<SourceSchema> {
        None  // default — existing sources compile unchanged
    }
}
```

This is a non-breaking change. Every existing source compiles without modification — the default returns `None`, meaning "this source doesn't describe its schema." Sources opt in by overriding the method.

### Schema Types

```rust
/// Schema information provided by a source.
pub struct SourceSchema {
    pub nodes: Vec<NodeSchema>,
    pub relations: Vec<RelationSchema>,
}

/// Schema for a node label.
pub struct NodeSchema {
    pub label: String,
    pub properties: Vec<PropertySchema>,
}

/// Schema for a relationship type.
pub struct RelationSchema {
    pub label: String,
    pub from: Option<String>,   // source node label
    pub to: Option<String>,     // target node label
    pub properties: Vec<PropertySchema>,
}

/// Schema for a single property.
pub struct PropertySchema {
    pub name: String,
    pub data_type: Option<PropertyType>,  // not all sources can provide types
    pub description: Option<String>,
}

/// Property data types.
pub enum PropertyType {
    String,
    Integer,
    Float,
    Boolean,
    Timestamp,
    Json,
}
```

### Per-Source Implementations

**PostgreSQL source** — the richest schema, introspected from the database:

The postgres source already connects to the database to set up replication. During that initial connection, it can query `information_schema.columns` for the subscribed tables and cache the result. `describe_schema()` then returns the cached schema at zero additional cost.

```rust
fn describe_schema(&self) -> Option<SourceSchema> {
    Some(SourceSchema {
        nodes: self.cached_table_schemas.iter().map(|t| NodeSchema {
            label: t.name.clone(),
            properties: t.columns.iter().map(|c| PropertySchema {
                name: c.name.clone(),
                data_type: Some(pg_type_to_property_type(&c.pg_type)),
                description: None,
            }).collect(),
        }).collect(),
        relations: vec![],
    })
}
```

**HTTP webhook source** — complete from config, since webhook mappings explicitly declare labels and properties in their templates:

```rust
fn describe_schema(&self) -> Option<SourceSchema> {
    Some(SourceSchema {
        nodes: self.config.mappings.iter()
            .filter(|m| m.element_type == ElementType::Node)
            .map(|m| NodeSchema {
                label: m.template.labels[0].clone(),
                properties: extract_property_names(&m.template.properties),
            }).collect(),
        relations: self.config.mappings.iter()
            .filter(|m| m.element_type == ElementType::Relation)
            .map(|m| RelationSchema {
                label: m.template.labels[0].clone(),
                from: m.template.from.clone(),
                to: m.template.to.clone(),
                properties: extract_property_names(&m.template.properties),
            }).collect(),
    })
}
```

**Mock source** — simple hardcoded schema per data type:

```rust
fn describe_schema(&self) -> Option<SourceSchema> {
    let (label, props) = match self.data_type {
        DataType::SensorReading => ("SensorReading", vec!["id", "temperature", "humidity", "timestamp"]),
        DataType::Counter => ("Counter", vec!["id", "value"]),
        DataType::Generic => ("Generic", vec!["id", "value", "message"]),
    };
    Some(SourceSchema {
        nodes: vec![NodeSchema {
            label: label.into(),
            properties: props.iter().map(|p| PropertySchema {
                name: p.to_string(), data_type: None, description: None,
            }).collect(),
        }],
        relations: vec![],
    })
}
```

**gRPC / Platform sources** — return `None` initially. These sources have externally-defined schemas. They can implement `describe_schema()` later if metadata becomes available (e.g., from protobuf reflection or Redis stream field inspection).

### How `get_schema` Uses It

The MCP tool becomes simple — it asks each source to describe itself and merges the results:

```rust
async fn get_schema(&self, instance_id: &str) -> Schema {
    let core = self.get_core(instance_id);
    let mut schema = Schema::new();

    // Each source describes its own schema
    for source_info in core.list_sources().await {
        if let Some(source_schema) = core.get_source_schema(&source_info.id).await {
            schema.merge(&source_info.id, source_schema);
        }
    }

    // Supplement with query-level info (labels from Cypher parsing)
    for query_config in persistence.list_queries(instance_id).await {
        if let Ok(labels) = LabelExtractor::extract_labels(
            &query_config.query, &query_config.query_language
        ) {
            schema.mark_labels_queried(labels, &query_config.id);
        }
    }

    schema
}
```

The `get_schema` tool doesn't parse config DTOs or guess at schema structure. Source-specific knowledge stays in the source. `LabelExtractor` adds one supplemental layer: which labels are actively being queried, so the agent knows what's already wired up.

### Why This Design

- **Extensible** — Third-party source plugins implement `describe_schema()` and they're automatically discoverable by any MCP client. No changes to drasi-server needed.
- **Accurate** — Each source knows its own schema. Postgres introspects real column types from the database. HTTP knows its webhook templates. External sources can fetch schemas from their upstream.
- **No reverse-engineering** — The MCP tool doesn't need to understand the internal config format of every source type.
- **Progressive** — Sources return `None` until they're ready. No pressure to implement immediately. The tool gracefully degrades.
- **Dynamic** — A postgres source that adds a new table at runtime can update its cached schema. The tool always returns current state.

### Example Output

```json
{
  "nodes": {
    "Order": {
      "source": "order-db",
      "properties": [
        { "name": "id", "type": "integer" },
        { "name": "total", "type": "float" },
        { "name": "status", "type": "string" },
        { "name": "customer_id", "type": "integer" },
        { "name": "created_at", "type": "timestamp" }
      ],
      "queriedBy": ["high-value-orders", "order-stats"]
    },
    "Sensor": {
      "source": "webhook-source",
      "properties": [
        { "name": "temperature", "type": null },
        { "name": "location", "type": null }
      ],
      "queriedBy": ["high-temp-alert"]
    }
  },
  "relations": {
    "PLACED": {
      "source": "order-db",
      "from": "Customer",
      "to": "Order",
      "properties": [],
      "queriedBy": ["customer-orders"]
    }
  },
  "sourcesWithoutSchema": ["grpc-events"]
}
```

The `sourcesWithoutSchema` field tells the AI agent that some sources exist but couldn't describe their schema, so there may be additional labels available beyond what's listed.

## MCP Resources

Resources are data endpoints that the AI agent can read (and optionally subscribe to for updates):

| Resource URI | Description |
|---|---|
| `drasi://instances` | List of all DrasiLib instances |
| `drasi://instances/{id}/schema` | Graph schema for an instance |
| `drasi://instances/{id}/queries/{queryId}/results` | Current result set for a query |
| `drasi://query-language/reference` | Drasi Cypher dialect reference (functions, clauses, operators) |
| `drasi://query-language/examples` | Categorized example query patterns |

### Resource Subscriptions

The MCP spec supports resource subscriptions where the server notifies the client when a resource changes. This maps naturally to Drasi's continuous query model:

- **`drasi://instances/{id}/queries/{queryId}/results`** — When subscribed, the server sends `notifications/resources/updated` whenever the query's result set changes. Internally, this uses the same `ApplicationReaction` mechanism that powers the `/attach` SSE endpoint.

This is the key real-time integration point: an AI agent subscribes to a query's results resource, and Drasi pushes change notifications through the MCP protocol whenever data changes. The agent doesn't need to poll.

## MCP Prompts (Agent Skills)

Prompts are the mechanism through which agent skills are delivered. Each prompt encodes a multi-step Drasi workflow with domain expertise.

### Skill: `watch_for_changes`

**Description**: Set up a continuous query to watch for specific data changes and react to them.

**Arguments**:
| Name | Required | Description |
|---|---|---|
| `description` | Yes | Natural language description of what to watch for |
| `reaction_type` | No | How to be notified: `subscribe` (default), `log`, `http`, `sse` |

**Prompt template returned to the agent**:

```
You are setting up a Drasi continuous query to detect data changes in real time.

## Available Data Schema
{result of get_schema tool}

## Drasi Cypher Extensions
Drasi extends standard Cypher with these functions for detecting change over time:

- drasi.changeDateTime(node) — Returns the timestamp when a node was last changed
  Example: max(drasi.changeDateTime(m)) AS LastChanged

- drasi.trueLater(condition, timestamp) — Evaluates to true when the wall clock
  reaches the given timestamp AND the condition is true. Use this to detect
  "something should have happened by now" scenarios.
  Example: WHERE drasi.trueLater(LastHeartbeat + duration({minutes: 5}))

- drasi.trueFor(condition, duration) — Evaluates to true when the condition has
  been continuously true for at least the specified duration.
  Example: WHERE drasi.trueFor(s.status = 'down', duration({minutes: 10}))

- datetime.realtime() — Current wall-clock time
- duration({seconds: N, minutes: N, hours: N, days: N}) — Duration literals

## Standard Cypher Supported
MATCH, WHERE, RETURN, WITH, ORDER BY, LIMIT, UNWIND
Aggregations: count(), sum(), avg(), min(), max()
Patterns: (n:Label), (a)-[:REL_TYPE]->(b)
Operators: =, <>, <, >, <=, >=, IN, CONTAINS, STARTS WITH, ENDS WITH, IS NULL, IS NOT NULL, AND, OR, NOT

## Example Patterns
- Threshold: MATCH (s:Sensor) WHERE s.temperature > 80 RETURN s
- Aggregation: MATCH (o:Order) RETURN count(o) AS Total, sum(o.amount) AS Revenue
- Multi-source join: MATCH (c:Customer), (o:Order) WHERE c.id = o.customer_id RETURN c.name, o.total
- Absence detection: MATCH (s:Service) WHERE drasi.trueFor(s.last_heartbeat < datetime.realtime() - duration({minutes: 5}), duration({minutes: 1})) RETURN s
- Time trigger: MATCH (t:Task) WITH t, drasi.changeDateTime(t) AS changed WHERE drasi.trueLater(changed + duration({hours: 24})) AND t.status = 'open' RETURN t

## Your Task
The user wants to watch for: "{description}"

Steps:
1. Examine the schema to identify relevant labels and properties
2. Write a Cypher query that matches the user's intent
3. Call the validate_query tool to check syntax and label references
4. If validation fails, fix the query and re-validate
5. Call create_query with autoStart: true
6. Subscribe to the query results resource for real-time notifications, or create a reaction of type "{reaction_type}" if specified
7. Confirm to the user: what you're watching for, which query was created, and how they'll be notified
```

### Skill: `explain_changes`

**Description**: Interpret and explain a set of changes from a Drasi query.

**Arguments**:
| Name | Required | Description |
|---|---|---|
| `query_id` | Yes | The query whose changes to explain |
| `changes` | Yes | The change event data to interpret |

**Prompt template**:

```
You are explaining data changes detected by a Drasi continuous query.

## Query Information
{result of get_query tool for query_id — includes query text and source info}

## Current Full Result Set
{result of get_query_results tool}

## Changes to Explain
{changes}

## Your Task
1. Identify what changed (added, updated, or deleted records)
2. For updates, describe what specific values changed (before → after)
3. Explain why this matters in the context of what the query is watching for
4. Provide a concise natural language summary suitable for a notification
5. If the change suggests an action should be taken, recommend it
```

### Skill: `diagnose_across_sources`

**Description**: Investigate a problem by correlating data across multiple sources.

**Arguments**:
| Name | Required | Description |
|---|---|---|
| `problem_description` | Yes | What problem or anomaly to investigate |

**Prompt template**:

```
You are investigating a problem by correlating data across multiple Drasi data sources.

## Available Data Schema
{result of get_schema tool — includes all sources and their labels/properties}

## Available Sources
{result of list_sources tool}

## Your Task
The user is investigating: "{problem_description}"

Investigation approach:
1. Identify which sources and labels are relevant to this problem
2. Create exploratory queries against individual sources to gather baseline data
3. Look for temporal correlations — did changes in one source coincide with changes in another?
4. If correlation is found, create a cross-source join query to continuously monitor it
5. Present your findings:
   - What data you examined
   - What correlations you found (or didn't find)
   - A hypothesis about root cause
   - Any continuous queries you set up for ongoing monitoring
```

### Skill: `setup_data_quality_guard`

**Description**: Create continuous data quality rules that alert on violations.

**Arguments**:
| Name | Required | Description |
|---|---|---|
| `rules` | Yes | Natural language description of data quality rules to enforce |

**Prompt template**:

```
You are setting up continuous data quality monitoring using Drasi.

## Available Data Schema
{result of get_schema tool}

## Your Task
The user wants to enforce these data quality rules: "{rules}"

For each rule:
1. Identify the relevant labels and properties from the schema
2. Write a Cypher query that matches VIOLATIONS of the rule (records that fail the check)
3. Validate the query
4. Create it with autoStart: true and enableBootstrap: true
   (bootstrap will immediately surface any existing violations)
5. Check query results — report any pre-existing violations found during bootstrap
6. Summarize all rules that are now active
```

### Skill: `build_live_dashboard`

**Description**: Create a suite of continuous queries for real-time monitoring of a domain.

**Arguments**:
| Name | Required | Description |
|---|---|---|
| `domain` | Yes | What domain or system to monitor |
| `metrics` | No | Specific metrics or KPIs of interest |

**Prompt template**:

```
You are building a real-time monitoring dashboard using Drasi continuous queries.

## Available Data Schema
{result of get_schema tool}

## Your Task
The user wants to monitor: "{domain}"
Specific metrics of interest: "{metrics}"

Create a comprehensive set of queries:
1. **KPI queries** — Aggregation queries for key metrics (counts, sums, averages)
2. **Alert queries** — Threshold-based queries that detect anomalies or critical conditions
3. **Activity queries** — Queries that surface recent changes or activity feeds
4. **Trend queries** — Queries that track changes over time (using drasi.changeDateTime)

For each query:
- Create it with autoStart: true
- Subscribe to its results resource
- After all queries are running, summarize the dashboard:
  - What metrics are being tracked
  - What alerts are configured
  - Current values for all KPIs (from initial results)
```

### Skill: `detect_absence`

**Description**: Set up monitoring for events that should happen but don't — Drasi's most unique capability.

**Arguments**:
| Name | Required | Description |
|---|---|---|
| `expected_event` | Yes | What event is expected to occur |
| `timeout` | Yes | How long to wait before alerting (e.g., "5 minutes", "1 hour") |

**Prompt template**:

```
You are setting up absence-of-change detection, which is one of Drasi's most powerful and unique capabilities. Unlike traditional systems that react to events, Drasi can detect when expected events DON'T occur.

## Available Data Schema
{result of get_schema tool}

## Key Drasi Temporal Functions
These functions are essential for absence detection:

- drasi.changeDateTime(node) — Timestamp of the node's last modification.
  Use this to track "when was this last updated?"

- drasi.trueLater(condition, timestamp) — Becomes true when the wall clock reaches
  the timestamp AND the base condition is met. This is the primary mechanism for
  "alert me if X hasn't happened by time T."
  Pattern: WHERE drasi.trueLater(drasi.changeDateTime(n) + duration({{timeout}}))

- drasi.trueFor(condition, duration) — Becomes true when a condition has been
  continuously true for the specified duration.
  Pattern: WHERE drasi.trueFor(s.status = 'waiting', duration({{timeout}}))

## Absence Detection Patterns

**Heartbeat monitoring** (service hasn't checked in):
```cypher
MATCH (s:Service)
WITH s, drasi.changeDateTime(s) AS lastSeen
WHERE drasi.trueLater(lastSeen + duration({{minutes: 5}}))
RETURN s.id AS ServiceId, lastSeen AS LastSeen
```

**Stale data** (record hasn't been updated):
```cypher
MATCH (r:Record)
WITH r, drasi.changeDateTime(r) AS lastUpdated
WHERE drasi.trueLater(lastUpdated + duration({{hours: 1}}))
RETURN r.id, lastUpdated
```

**Timeout** (task stuck in a state):
```cypher
MATCH (t:Task)
WHERE t.status = 'processing'
WITH t, drasi.changeDateTime(t) AS stateEntered
WHERE drasi.trueLater(stateEntered + duration({{minutes: 30}}))
RETURN t.id, t.status, stateEntered
```

## Your Task
The user expects: "{expected_event}"
Timeout threshold: "{timeout}"

1. Map the expected event to schema labels and properties
2. Choose the appropriate absence detection pattern
3. Write the Cypher query using drasi.trueLater or drasi.trueFor
4. Validate and create the query
5. Explain to the user exactly how the detection works and when they'll be alerted
```

## Server Instructions

The MCP `ServerInfo.instructions` field is the first thing an AI agent sees when connecting. It provides global context about what this server does:

```
Drasi is a real-time change detection engine. It continuously monitors data sources
(databases, APIs, webhooks), evaluates graph queries against incoming changes, and
triggers reactions when query results change.

Key concepts:
- Sources: Data inputs (PostgreSQL CDC, HTTP webhooks, gRPC streams)
- Queries: Continuous Cypher/GQL graph queries that maintain a live result set
- Reactions: Automated responses when query results change (webhooks, SSE, logging)

Drasi's unique capability is detecting the ABSENCE of change — alerting when
expected events don't occur within a time window, using temporal functions like
drasi.trueLater() and drasi.trueFor().

Use the get_query_context tool to discover available data and query syntax before
creating queries. Use prompts (skills) for guided workflows.
```

## Configuration

MCP mode uses the same YAML config file as `run` mode. No additional configuration is needed:

```bash
# Run as HTTP server (existing behavior)
drasi-server run --config config/server.yaml

# Run as MCP server over stdio (new)
drasi-server mcp --config config/server.yaml
```

### Client Configuration Examples

**Claude Desktop** (`claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "drasi": {
      "command": "drasi-server",
      "args": ["mcp", "--config", "/path/to/config/server.yaml"]
    }
  }
}
```

**VS Code Copilot** (`.vscode/mcp.json`):
```json
{
  "servers": {
    "drasi": {
      "command": "drasi-server",
      "args": ["mcp", "--config", "${workspaceFolder}/config/server.yaml"]
    }
  }
}
```

**Cursor** (Settings → MCP):
```json
{
  "mcpServers": {
    "drasi": {
      "command": "drasi-server",
      "args": ["mcp", "--config", "/path/to/config/server.yaml"]
    }
  }
}
```

## Implementation Plan

### Phase 1: Core MCP Server (Foundation)

**Goal**: `drasi-server mcp` starts, exposes tools, handles JSON-RPC over stdio.

1. Add `rmcp` dependency with `server` and `transport-io` features
2. Add `Mcp` variant to the `Commands` enum in `main.rs`
3. Create `src/mcp/` module with:
   - `mod.rs` — Module structure
   - `server.rs` — `DrasiMcpServer` struct with `#[tool_router]` / `#[tool_handler]`
   - `tools/sources.rs` — Source management tools
   - `tools/queries.rs` — Query management tools
   - `tools/reactions.rs` — Reaction management tools
   - `tools/discovery.rs` — Schema discovery and query context tools
4. Implement `run_mcp_server()` in `main.rs` that initializes DrasiLib and serves via `stdio()`
5. Ensure all logging goes to stderr (not stdout)

**Tools delivered**: `list_sources`, `create_source`, `delete_source`, `start_source`, `stop_source`, `list_queries`, `create_query`, `delete_query`, `start_query`, `stop_query`, `get_query_results`, `list_reactions`, `create_reaction`, `delete_reaction`, `start_reaction`, `stop_reaction`, `list_instances`, `get_health`

### Phase 2: Schema Discovery & Validation

**Goal**: AI agents can discover available data and validate queries before creating them.

1. Add `SourceSchema`, `NodeSchema`, `RelationSchema`, `PropertySchema`, and `PropertyType` types to drasi-lib
2. Add `fn describe_schema(&self) -> Option<SourceSchema>` default method to the `Source` trait
3. Implement `describe_schema()` on each source plugin:
   - **PostgreSQL**: Introspect `information_schema.columns` during initial connection, cache result
   - **HTTP webhook**: Extract labels and property names from `ElementTemplateDto` mappings
   - **Mock**: Hardcoded schema per `DataType` variant
   - **gRPC / Platform**: Return `None` (implement later when metadata is available)
4. Add `get_source_schema(id)` method to `DrasiLib` that calls `describe_schema()` on the source instance
5. Implement `get_schema` MCP tool — iterates sources, merges schemas, supplements with `LabelExtractor` for query info
6. Implement `get_query_context` MCP tool — combines schema + language reference + examples
7. Implement `validate_query` MCP tool — parses Cypher, checks label references against schema
8. Add static Drasi Cypher reference data (functions, clauses, operators)
9. Add categorized example query library

**Tools delivered**: `get_schema`, `get_query_context`, `validate_query`

### Phase 3: Resources & Subscriptions

**Goal**: AI agents can read and subscribe to live data.

1. Implement MCP resource handlers for schema, query results, language reference
2. Implement resource subscriptions for query results (backed by `ApplicationReaction`)
3. Send `notifications/resources/updated` when subscribed query results change

**Resources delivered**: All resources listed in the Resources section above.

### Phase 4: Agent Skills (Prompts)

**Goal**: AI agents get guided workflows for common Drasi patterns.

1. Implement MCP prompt handlers for each skill
2. Each prompt dynamically injects current schema and context
3. Skills delivered: `watch_for_changes`, `explain_changes`, `diagnose_across_sources`, `setup_data_quality_guard`, `build_live_dashboard`, `detect_absence`

### Phase 5: Polish & Documentation

1. End-to-end testing with Claude Desktop, VS Code Copilot, and Cursor
2. Documentation: README section on MCP mode, client configuration examples
3. Example configs optimized for MCP usage (pre-configured sources, ready for queries)

## File Structure

```
src/
├── main.rs                          # Add Mcp command variant
├── mcp/
│   ├── mod.rs                       # Module exports
│   ├── server.rs                    # DrasiMcpServer struct, ServerHandler impl
│   ├── tools/
│   │   ├── mod.rs
│   │   ├── sources.rs               # Source management tools
│   │   ├── queries.rs               # Query management tools
│   │   ├── reactions.rs             # Reaction management tools
│   │   └── discovery.rs             # Schema, query context, validation tools
│   ├── resources/
│   │   ├── mod.rs
│   │   ├── schema.rs                # Schema resource
│   │   ├── query_results.rs         # Query results resource + subscriptions
│   │   └── reference.rs             # Language reference & examples resources
│   └── prompts/
│       ├── mod.rs
│       ├── watch_for_changes.rs     # Watch skill
│       ├── explain_changes.rs       # Explain skill
│       ├── diagnose.rs              # Diagnose skill
│       ├── data_quality.rs          # Data quality skill
│       ├── dashboard.rs             # Dashboard skill
│       └── absence_detection.rs     # Absence detection skill
├── server.rs                        # Existing HTTP server (unchanged)
├── api/                             # Existing REST API (unchanged)
└── ...
```

## Feature Flag

The MCP mode should be behind a cargo feature flag to keep it optional and avoid adding the `rmcp` dependency for users who only need the HTTP API:

```toml
[features]
default = []
mcp = ["rmcp"]

[dependencies]
rmcp = { version = "0.3", features = ["server", "transport-io"], optional = true }
```

The `Mcp` CLI command is conditionally compiled:

```rust
#[derive(Subcommand)]
enum Commands {
    Run { /* ... */ },
    Validate { /* ... */ },
    Doctor { /* ... */ },
    Init { /* ... */ },
    #[cfg(feature = "mcp")]
    Mcp {
        #[arg(short, long, default_value = "config/server.yaml")]
        config: PathBuf,
    },
}
```

## Relationship to HTTP API

MCP mode and HTTP mode are **complementary, not competing**:

- **HTTP mode** (`drasi-server run`): For production deployments, dashboards, programmatic REST clients, and browser-based UIs. Continues to be the primary deployment mode.
- **MCP mode** (`drasi-server mcp`): For AI agent integration during development, interactive exploration, and agentic workflows. Designed to be launched as a subprocess by AI tools.

Both modes share the same DrasiLib core, config format, and plugin system. Code written for tools in MCP mode can reuse the same handler logic that powers the REST API.

## Success Criteria

1. `drasi-server mcp --config config/server.yaml` starts and completes MCP handshake over stdio
2. Claude Desktop can discover all Drasi tools and call them successfully
3. An AI agent can go from zero knowledge to a working continuous query using only `get_query_context` → `validate_query` → `create_query` → `get_query_results`
4. The `watch_for_changes` prompt successfully guides an agent through the full workflow
5. Resource subscriptions deliver real-time query result changes to the connected agent
6. The `detect_absence` prompt successfully teaches an agent to use `drasi.trueLater`/`drasi.trueFor`
