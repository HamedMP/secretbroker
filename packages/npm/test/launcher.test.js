import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const launcher = fileURLToPath(new URL("../bin/secretbroker.js", import.meta.url));

// CI and local test runners provide a freshly built synthetic binary path.
test("launcher executes the selected native binary", () => {
  assert.ok(process.env.SECRETBROKER_BINARY, "SECRETBROKER_BINARY is required for this test");
  const result = spawnSync(process.execPath, [launcher, "--version"], {
    encoding: "utf8",
    env: process.env,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^secretbroker 0\.1\.0/m);
});
