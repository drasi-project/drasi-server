import assert from "node:assert/strict";
import test from "node:test";
import { instancePath, requireInstanceId } from "../src/api/paths.ts";

test("component operations require a selected instance", () => {
  for (const id of [undefined, ""]) {
    assert.throws(() => requireInstanceId(id), /Select an instance/);
    assert.throws(() => instancePath(id, "queries"), /Select an instance/);
  }
});

test("scoped paths encode each identity exactly once", () => {
  assert.equal(instancePath("first", "queries"), "/instances/first/queries");
  assert.equal(
    instancePath("tenant/a %", "queries", "q?/#", "attach"),
    "/instances/tenant%2Fa%20%25/queries/q%3F%2F%23/attach",
  );
  assert.equal(instancePath("tenant%2Fa"), "/instances/tenant%252Fa");
});

test("empty identifiers and dot segments cannot escape their instance scope", () => {
  for (const id of ["", ".", ".."]) {
    assert.throws(() => instancePath("selected", "queries", id), /Invalid/);
  }
  for (const id of [".", ".."]) {
    assert.throws(() => instancePath(id, "queries"), /Invalid/);
  }
});
