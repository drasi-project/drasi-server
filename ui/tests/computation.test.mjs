import assert from "node:assert/strict";
import test from "node:test";
import { buildFlowGraph, getNodeDimensions, layoutFlowNodes } from "../src/utils/graph.ts";
import { computationStatus, isInternalComponent } from "../src/utils/computation.ts";

function component(id, role, overrides = {}) {
  return {
    id, role, implementation: null, ports: [], autoStart: false,
    realization: "Created", lifecycle: "Running", health: "Unknown", failurePhase: null,
    ...overrides,
  };
}

function native(from, to, port = "out", overrides = {}) {
  return {
    representation: "NativeProvider", from: { component: from, port },
    to: { component: to, port: "in" }, binding: "Bound", availability: "Available",
    ...overrides,
  };
}

function pipeline(components, relationships = []) {
  return {
    computation: {
      graph: { id: "test", revision: 1, state: "Running", driverFailed: false },
      components, relationships, resources: [],
    },
    sources: [{ id: "source", kind: "mock", status: "Running", autoStart: true }],
    queries: [{ id: "query", status: "Running", sourceIds: ["not-a-real-edge"] }],
    reactions: [{ id: "reaction", kind: "log", status: "Running", queryIds: ["query"] }],
  };
}

const implementation = (name) => ({ name, version: "1", plugin: null });

test("all semantic kinds and unknown future kinds render, preserving legacy renderers", () => {
  const data = pipeline([
    component("source", "Source"), component("query", "Query"), component("reaction", "Reaction"),
    component("transformer", "Transformer"), component("sink", "Sink"), component("service", "Service"),
    component("future", "FutureKind"),
  ], [native("source", "transformer"), native("transformer", "query"), native("query", "sink")]);
  const graph = buildFlowGraph(data);
  assert.equal(graph.nodes.length, 7);
  assert.deepEqual(graph.nodes.map((n) => n.type),
    ["sourceNode", "queryNode", "reactionNode", "computationNode", "computationNode", "computationNode", "computationNode"]);
  assert.equal(graph.edges.length, 3, "do not infer extra edges from legacy sourceIds/queryIds");
  assert.equal(new Set(graph.nodes.map((n) => n.id)).size, 7);
});

test("queries and transformers share a column across processing stages and renderer types", () => {
  const data = pipeline([
    component("source", "Source"), component("before", "Transformer"), component("query", "Query"),
    component("after", "Transformer"), component("native-query", "Query"), component("sink", "Sink"),
    component("__internal-query", "Query"),
  ], [
    native("source", "before"), native("before", "query"), native("query", "after"),
    native("after", "native-query"), native("native-query", "sink"),
    native("source", "__internal-query"), native("__internal-query", "after"),
  ]);
  for (const showInternal of [false, true]) {
    const graph = buildFlowGraph(data, showInternal);
    const processing = (nodes) => nodes.filter((n) => !n.hidden && ["Query", "Transformer"].includes(n.data.role));
    assert.equal(processing(graph.nodes).length, showInternal ? 5 : 4);
    assert.equal(new Set(processing(graph.nodes).map((n) => n.position.x)).size, 1);
    assert.equal(graph.edges.length, data.computation.relationships.length, "same-column connections remain intact");
    assert.ok(graph.nodes[0].position.x < graph.nodes[1].position.x);
    assert.ok(graph.nodes[5].position.x > graph.nodes[1].position.x);

    const moved = graph.nodes.map((n, i) => ({
      ...n, position: { x: i * 700, y: 0 }, data: { ...n.data, expanded: n.data.id === "query" },
    }));
    const arranged = layoutFlowNodes(moved, graph.edges);
    const column = processing(arranged);
    assert.equal(new Set(column.map((n) => n.position.x)).size, 1, "Auto-layout uses the same shared column");
    for (let i = 1; i < column.length; i++) {
      assert.ok(column[i].position.y >= column[i - 1].position.y + getNodeDimensions(column[i - 1]).height);
    }
    assert.ok(arranged[5].position.x >= column[0].position.x + 420, "outputs clear expanded query cards");
  }
});

test("application view hides only recognized runtime helpers and internal namespace", () => {
  const helpers = [
    component("__component_graph__", "Source"),
    component("any-subscription-id", "Source", { implementation: implementation("drasi/source-plugin-subscription") }),
    component("any-schedule-id", "Source", { implementation: implementation("drasi/query-scheduled-source") }),
    component("any-outlet-id", "Sink", { implementation: implementation("drasi/query-results-outlet") }),
  ];
  const ordinary = component("scheduled/user-defined-name", "Source");
  const data = pipeline([component("query", "Query"), ordinary, ...helpers], [
    native("any-schedule-id", "query"), native(ordinary.id, "query"),
  ]);
  const application = buildFlowGraph(data);
  assert.equal(application.nodes.filter((n) => !n.hidden).length, 2);
  assert.equal(application.edges.filter((e) => !e.hidden).length, 1);
  assert.equal(isInternalComponent(ordinary), false, "names resembling helpers are not sufficient");
  const full = buildFlowGraph(data, true);
  assert.equal(full.nodes.filter((n) => !n.hidden).length, 6);
  assert.equal(full.edges.filter((e) => !e.hidden).length, 2);
  assert.deepEqual(application.nodes.map((n) => n.id), full.nodes.map((n) => n.id));
});

test("pipe ports, subscriptions and control connections retain distinct identities", () => {
  const data = pipeline([component("source", "Source"), component("query", "Query")], [
    native("source", "query", "one"),
    native("source", "query", "two", { availability: "Idle" }),
    { representation: "HostSubscription", from: "source", to: "query", producerStarted: true, consumerStarted: true },
    { representation: "ControlConnection", from: "query", to: "source" },
  ]);
  const graph = buildFlowGraph(data);
  assert.equal(new Set(graph.edges.map((e) => e.id)).size, 4);
  assert.equal(graph.edges[0].animated, true);
  assert.equal(graph.edges[1].animated, false);
  assert.equal(graph.edges[2].animated, false, "started is not evidence of flowing data");
  assert.equal(graph.edges[3].style.strokeDasharray, "5 5");
  assert.match(graph.edges[0].label, /one -> in/);
});

test("unresolved relationships are reported rather than creating phantom components", () => {
  const graph = buildFlowGraph(pipeline([component("query", "Query")], [native("missing", "query")]));
  assert.equal(graph.nodes.length, 1);
  assert.equal(graph.edges.length, 0);
  assert.equal(graph.unresolvedRelationships, 1);
});

test("a new topology removes obsolete edges and preserves component identity across metadata fetches", () => {
  const before = pipeline([component("query", "Query"), component("transformer", "Transformer")], [native("query", "transformer")]);
  const after = pipeline([component("query", "Query")]);
  const initial = buildFlowGraph({ ...before, queries: [] });
  const loaded = buildFlowGraph(before);
  assert.equal(initial.nodes[0].id, loaded.nodes[0].id);
  assert.equal(buildFlowGraph(after).edges.length, 0);
  assert.equal(buildFlowGraph(after).nodes.length, 1);
});

test("lifecycle and realization are not silently reported as stopped or running", () => {
  for (const [overrides, expected] of [
    [{ realization: "CreationFailed" }, "Error"],
    [{ lifecycle: "Failed" }, "Error"],
    [{ realization: "Blocked", lifecycle: "Stopped" }, "Blocked"],
    [{ realization: "Pending", lifecycle: "Stopped" }, "Pending"],
    [{ realization: "Creating", lifecycle: "Stopped" }, "Creating"],
    [{ lifecycle: "Quiescing" }, "Quiescing"],
    [{ lifecycle: "Quiesced" }, "Quiesced"],
    [{ realization: null, lifecycle: null }, "Unknown"],
    [{ lifecycle: "NewState" }, "Unknown"],
  ]) assert.equal(computationStatus(component("test", "Service", overrides)), expected);
});

test("layout handles cycles, disconnected components and variable card dimensions", () => {
  const data = pipeline([
    component("a", "Transformer"), component("b", "Query"), component("c", "Service"),
    component("d", "Source"), component("e", "Sink"),
  ], [native("a", "b"), native("b", "a"), native("d", "e")]);
  const graph = buildFlowGraph(data, true);
  const dimensions = (n) => ({ width: n.id === "component-a" ? 420 : 220, height: 280 });
  const positioned = layoutFlowNodes(graph.nodes, graph.edges, dimensions);
  assert.equal(positioned.length, 5);
  assert.deepEqual(positioned, layoutFlowNodes(graph.nodes, graph.edges, dimensions));
  for (const a of positioned) for (const b of positioned) {
    if (a.id === b.id) continue;
    assert.ok(a.position.x + dimensions(a).width <= b.position.x
      || b.position.x + dimensions(b).width <= a.position.x
      || a.position.y + dimensions(a).height <= b.position.y
      || b.position.y + dimensions(b).height <= a.position.y);
  }
});

test("absence of inspection does not silently fall back to a partial legacy graph", () => {
  assert.equal(buildFlowGraph({ ...pipeline([]), computation: undefined }).nodes.length, 0);
});

test("expanded dimensions survive layout and hidden nodes keep their own position", () => {
  const { nodes, edges } = buildFlowGraph(pipeline([
    component("query", "Query"), component("other-query", "Query"), component("__hidden", "Service"),
  ]));
  nodes[0].data.expanded = true;
  nodes[2].position = { x: 777, y: 888 };
  const positioned = layoutFlowNodes(nodes, edges);
  assert.deepEqual(getNodeDimensions(nodes[0]), { width: 420, height: 280 });
  assert.ok(positioned[1].position.y >= positioned[0].position.y + 280);
  assert.deepEqual(positioned[2].position, { x: 777, y: 888 });
});
