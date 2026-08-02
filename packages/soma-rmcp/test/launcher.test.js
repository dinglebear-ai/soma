"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const test = require("node:test");
const { binaryPath, installRoot } = require("../lib/platform");

test("launcher executes the package-installed native binary", () => {
  const destination = binaryPath();
  fs.rmSync(installRoot(), { recursive: true, force: true });
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  const script = [
    "#!/bin/sh",
    "printf '%s' 'soma-package-binary'",
  ].join(String.fromCharCode(10));
  fs.writeFileSync(destination, script, { mode: 0o755 });
  try {
    const result = spawnSync(process.execPath, [path.resolve(__dirname, "../bin/soma-rmcp.js"), "--package-smoke"], {
      encoding: "utf8",
    });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, "soma-package-binary");
  } finally {
    fs.rmSync(installRoot(), { recursive: true, force: true });
  }
});
