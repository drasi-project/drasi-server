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

const source = `Typed scene nodes (labels: kind label + SceneObject)
Cart { id, kind: "cart", name: string, active: boolean
  radius: number; clearance: number }
Journey { id, kind: "journey", name: string, active: boolean
  cart_id: string; destination_id: string; points: Point[] }
Destination { id, kind: "destination", name, active, point: Point }
Obstacle { id, kind: "obstacle", name, active, vertices: Point[] }

SceneStatus { id: "current"; revision: u64; command: string
  changed: string[]; objects: integer }
Point = [x: number, y: number]`;

const footprints = `Query: journey-footprints → JourneyFootprint {
  journey_id: string; cart_id: string; points: Point[]
  radius: number; clearance: number
}
Query: obstacle-footprints → ObstacleFootprint {
  obstacle_id: string; vertices: Point[]
}`;

const obstruction = `Obstruction {
  id: string // journey_id/obstacle_id
  journey_id: string; cart_id: string
  obstacle_id: string
  distance_m: number; required_m: number
}`;

const sceneStatus = `SceneStatus {
  id: "current"; revision: u64; command: string
  changed: string[]; objects: integer
}`;

const sceneRow = `SceneInput {
  id: string; kind: string; name: string; active: boolean
  cart_id, destination_id: string | null
  points, vertices: Point[] | null; point: Point | null
  radius, clearance: number | null
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
Element ID: ["relation","obstruction",journey_id,obstacle_id]`;

const rows = `Row values, selected by query ID:

scene-inputs:
${sceneRow}

journey-footprints / obstacle-footprints:
${footprints}

obstructions:
${obstruction}

affected-journeys:
${impactRow}

scene-status:
${sceneStatus}`;

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
    title: 'Scene store → Footprint queries',
    implementation: 'scene.out → journey-footprints.in / obstacle-footprints.in · drasi.graph-change v1',
    detail: graphDetail, schema: source,
    note: `${graphNote} Active filtering and the journey-to-cart join happen in the receiving queries; the source stream also contains inactive objects and the SceneStatus node.`,
  },
  {
    id: 'context-geometry', kind: 'rows', path: 'M773 210 H827',
    title: 'Footprint queries → Geometry',
    implementation: 'journey-footprints.out / obstacle-footprints.out → geometry.in · drasi.graph-change v1 (Result nodes)',
    detail: queryDetail, schema: footprints,
    note: `${queryNote} Geometry keeps a footprint per Result node and recomputes only the pairs a changed footprint touches.`,
  },
  {
    id: 'geometry-impact', kind: 'graph', path: 'M1033 210 H1087',
    title: 'Geometry → Affected journeys',
    implementation: 'geometry.out → affected-journeys.in · drasi.graph-change v1',
    detail: graphDetail, schema: geometry,
    note: `${graphNote} Each Obstruction is inserted, updated or deleted as its journey/obstacle pair changes; the query joins it to the scene's names.`,
  },
  {
    id: 'scene-impact', kind: 'graph', path: 'M450 152 V130 H1150 V152',
    title: 'Scene store → Affected journeys',
    implementation: 'scene.out → affected-journeys.in · drasi.graph-change v1',
    detail: graphDetail, schema: source,
    note: `${graphNote} The impact query reads the typed Cart, Journey, Destination and Obstacle nodes directly, so a name-only edit updates the impact without recomputing geometry.`,
  },
  {
    id: 'scene-inspection', kind: 'graph', path: 'M410 268 V405 H567',
    title: 'Scene store → Input/status inspection',
    implementation: 'scene.out → scene-inputs.in / scene-status.in · drasi.graph-change v1',
    detail: graphDetail, schema: source,
    note: `${graphNote} This is the same source stream sent to the footprint queries; scene-inputs retains inactive objects too, and scene-status selects the SceneStatus node.`,
  },
  {
    id: 'geometry-inspection', kind: 'graph', path: 'M930 268 V315 H670 V347',
    title: 'Geometry → Cause inspection',
    implementation: 'geometry.out → obstructions.in',
    detail: graphDetail, schema: geometry,
    note: `${graphNote} The obstructions query receives the same drasi.graph-change v1 stream as the impact query and selects Obstruction nodes.`,
  },
  {
    id: 'context-results', kind: 'rows', path: 'M670 152 V110 H1320 V405 H1293',
    title: 'Footprint queries → Query results outlet',
    implementation: 'journey-footprints.out / obstacle-footprints.out → query-results.in · drasi.graph-change v1 (Result nodes)',
    detail: queryDetail, schema: footprints,
    note: `${queryNote} These are the same outputs that feed Geometry, published separately for inspection.`,
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
    schema: `scene-inputs Result node properties:
${sceneRow}

obstructions Result node properties:
${obstruction}

scene-status Result node properties:
${sceneStatus}`,
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
    implementation: 'All six query out ports → wall-ui.in · drasi.graph-change v1 (Result nodes)',
    detail: 'Six independent bounded graph pipes, grouped in this arrow. The native sink reads these graph-change envelopes directly and writes the browser’s result-difference JSON only at the transport boundary, without catalog subscriptions or a reaction queue.',
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
