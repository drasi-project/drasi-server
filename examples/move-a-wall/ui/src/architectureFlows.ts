export type Connection = {
  id: string;
  kind: 'graph' | 'rows' | 'transport';
  path: string;
  title: string;
  implementation: string;
  detail: string;
  schema: string;
  note: string;
};

// Readable contract summaries, not JSON wire envelopes or live sample data.
const entity = `Entity {
  id: string; name: string
  active: boolean; shape: Shape
}
Shape =
  { kind: "cart", radius: number, clearance: number }
  | { kind: "journey", cart_id: string,
      destination_id: string, points: Point[] }
  | { kind: "destination", point: Point }
  | { kind: "obstacle", vertices: Point[] }
Point = [x: number, y: number]`;

const inputRecord = `InputRecord =
  { type: "entity", revision: u64, entity: Entity }
  | { type: "clock", revision: u64,
      members: Map<string, u64>,
      changed: string[], command: string }

${entity}`;

const source = `SceneObject node properties {
  id: string
  active: boolean
  payload: string // JSON-encoded InputRecord
}

${inputRecord}`;

const context = `Query: geometry-context
Result node properties (RETURN aliases) { objects: string[] }
Each string is a JSON-encoded InputRecord.

${inputRecord}`;

const obstruction = `Obstruction {
  id: string // journey_id/obstacle_id
  journey_id: string; cart_id: string
  obstacle_id: string
  distance_m: number; required_m: number
}`;

const status = `GeometryStatus {
  id: "current"; revision: u64
  objects: integer; obstructions: integer
}`;

const impactFields = `  id: string
  cart_id: string; cart: string
  journey_id: string; task: string
  destination_id: string; destination: string
  obstacle_id: string; obstacle: string
  distance_m: number; required_m: number
}`;
const impact = `AffectedJourney Result node properties {
${impactFields}`;
const impactRow = `AffectedJourney row {
${impactFields}`;

const geometry = `${obstruction}
${status}

Metadata nodes:
Cart { id, name, cart_id: string }
Journey {
  id, name, journey_id,
  cart_id, destination_id: string
}
Destination { id, name: string }
Obstacle { id, name, obstacle_id: string }`;

const rows = `Row values, selected by query ID:

scene-inputs / geometry-context:
  { objects: string[] } // JSON-encoded InputRecords

obstructions:
${obstruction}

affected-journeys:
${impactRow}

geometry-status:
${status}`;

const diffs = `ResultDiff =
  { type: "ADD" | "DELETE",
    data: Row, row_signature: u64 }
  | { type: "UPDATE", data: Row,
      before: Row, after: Row,
      grouping_keys?: string[], row_signature: u64 }
  | { type: "aggregation", before: Row | null,
      after: Row, row_signature: u64 }
  | { type: "noop" }`;

const graphDetail = 'Graph pipes carry graph-change v1 envelopes: ElementImage adds and replacements, ElementPatch updates and deletes. The schema below shows decoded node properties, not a JSON transport envelope.';
const graphNote = 'Inserts and replacements carry node properties. Source deletes can be identity-only (element identity and metadata, not the old properties). Envelopes carry stream/sequence and timing information.';
const queryDetail = 'A query output uses the same graph-change v1 schema as every other pipe. Each output is a Result node whose properties are the query’s RETURN aliases, with an identity scoped to that query output. Query ID, row kind, generation, sequence and snapshot markers are scoped envelope metadata, not node properties. The schema below shows the RETURN-alias properties.';
const queryNote = 'An added Result node carries an ElementImage; an update carries an ElementImage or ElementPatch; a delete carries the prior Result node as an ElementBeforeImage. A downstream transformer or query reads these envelopes directly, with no conversion step.';

export const connections: Connection[] = [
  {
    id: 'commands', kind: 'transport', path: 'M115 347 V210 H307',
    title: 'React UI → Scene store',
    implementation: 'POST /commands · application/json',
    detail: 'The browser sends a revision-checked input command. The source validates it before publishing graph changes.',
    schema: `Request {
  expected_revision: u64
  command:
    { action: "put", entity: Entity }
    | { action: "delete", id: string }
    | { action: "reset" | "clear" }
}
Success { accepted_revision: u64 }

${entity}`,
    note: 'Requires X-Wall-Command: 1. Stale revisions and invalid scene values are rejected. This request contains inputs, never computed obstructions.',
  },
  {
    id: 'scene-context', kind: 'graph', path: 'M513 210 H567',
    title: 'Scene store → Active context',
    implementation: 'scene.out → geometry-context.in · drasi.graph-change v1',
    detail: graphDetail, schema: source,
    note: `${graphNote} Active filtering happens in the receiving query; the source stream also contains inactive objects and the __clock manifest.`,
  },
  {
    id: 'context-geometry', kind: 'rows', path: 'M773 210 H827',
    title: 'Active context → Geometry',
    implementation: 'geometry-context.out → geometry.in · drasi.graph-change v1 (Result nodes)',
    detail: queryDetail, schema: context,
    note: `${queryNote} The collected objects include active entities and the clock manifest, which proves input revision completeness; geometry reads this output directly.`,
  },
  {
    id: 'geometry-impact', kind: 'graph', path: 'M1033 210 H1087',
    title: 'Geometry → Affected journeys',
    implementation: 'geometry.out → affected-journeys.in · drasi.graph-change v1',
    detail: graphDetail, schema: geometry,
    note: `${graphNote} Metadata nodes let the receiving query join an obstruction to cart, journey, destination and obstacle names.`,
  },
  {
    id: 'scene-inspection', kind: 'graph', path: 'M410 268 V405 H567',
    title: 'Scene store → Input inspection',
    implementation: 'scene.out → scene-inputs.in · drasi.graph-change v1',
    detail: graphDetail, schema: source,
    note: `${graphNote} This is the same source stream sent to Active context; scene-inputs retains inactive objects too.`,
  },
  {
    id: 'geometry-inspection', kind: 'graph', path: 'M930 268 V315 H670 V347',
    title: 'Geometry → Cause/status inspection',
    implementation: 'geometry.out → obstructions.in / geometry-status.in',
    detail: graphDetail, schema: geometry,
    note: `${graphNote} Both queries receive the same complete drasi.graph-change v1 stream, then select Obstruction or GeometryStatus nodes.`,
  },
  {
    id: 'context-results', kind: 'rows', path: 'M670 152 V110 H1320 V405 H1293',
    title: 'Active context → Query results outlet',
    implementation: 'geometry-context.out → query-results.in · drasi.graph-change v1 (Result nodes)',
    detail: queryDetail, schema: context,
    note: `${queryNote} This is the same output that feeds Geometry, published separately for inspection.`,
  },
  {
    id: 'impact-results', kind: 'rows', path: 'M1190 268 V347',
    title: 'Affected journeys → Query results outlet',
    implementation: 'affected-journeys.out → query-results.in · drasi.graph-change v1 (Result nodes)',
    detail: queryDetail, schema: impact,
    note: `${queryNote} One Result node represents one journey/obstacle cause. The query supplies the human-readable names.`,
  },
  {
    id: 'inspection-results', kind: 'rows', path: 'M773 405 H1087',
    title: 'Inspection queries → Query results outlet',
    implementation: 'Three independent out ports → query-results.in · drasi.graph-change v1 (Result nodes)',
    detail: queryDetail,
    schema: `scene-inputs Result node properties { objects: string[] }
Each string is a JSON-encoded InputRecord.

obstructions Result node properties:
${obstruction}

geometry-status Result node properties:
${status}`,
    note: `${queryNote} This arrow groups three independent feeds; it is not one combined result or atomic multi-query snapshot.`,
  },
  {
    id: 'catalog-api', kind: 'rows', path: 'M1190 463 V490 H410 V522',
    title: 'Shared result catalog → REST API',
    implementation: 'wall-query-catalog · retained query readers',
    detail: 'Shared-resource access, not a graph pipe or an output port on the sink. The API obtains the selected query’s retained results through its query reader and serves them as row JSON.',
    schema: rows,
    note: 'The query-results sink publishes changes; it does not create another copy of business state. HTTP snapshot JSON is shown on the REST API → React UI arrow.',
  },
  {
    id: 'catalog-sse', kind: 'rows', path: 'M670 463 V522',
    title: 'Continuous queries → native SSE sink',
    implementation: 'All five query out ports → wall-ui.in · drasi.graph-change v1 (Result nodes)',
    detail: 'Five independent bounded graph pipes, grouped in this arrow. The native sink reads these graph-change envelopes directly and writes the browser’s result-difference JSON only at the transport boundary, without catalog subscriptions or a reaction queue.',
    schema: queryDetail,
    note: 'The result shape is the selected query’s RETURN aliases, shown on its incoming arrow. Native envelopes include sequence; the unchanged browser SSE payload does not.',
  },
  {
    id: 'api-browser', kind: 'transport', path: 'M307 580 H115 V463',
    title: 'REST API → React UI',
    implementation: 'GET /api/v1/instances/move-a-wall/queries/:id/results',
    detail: 'The SDK reads an ordinary JSON snapshot for one query. This external API response contains current row JSON, not change operations or graph-change envelopes.',
    schema: `Successful response {
  success: true
  data: Row[]
  error: null
}
${rows}`,
    note: 'An empty result is data: []. Queries are read independently; this is not an atomic snapshot of the whole scene and all derived results.',
  },
  {
    id: 'sse-browser', kind: 'transport', path: 'M670 638 V660 H75 V463',
    title: 'Native SSE sink → React UI',
    implementation: 'GET /events · text/event-stream',
    detail: 'Each SSE data frame contains the standard, untemplated JSON payload below, produced by the sink from graph-change query outputs. The SDK applies its result differences to the selected query.',
    schema: `Result message {
  queryId: string
  results: ResultDiff[]
  timestamp: integer // milliseconds
}
${diffs}

Heartbeat { type: "heartbeat", ts: integer }`,
    note: 'This external SSE format is row JSON with ResultDiff events, not the internal graph-change envelope. Row is the selected query’s result shape, not a scene command. Unlike sparse source deletes, SSE DELETE includes the removed row. This default payload does not expose query sequence.',
  },
];
