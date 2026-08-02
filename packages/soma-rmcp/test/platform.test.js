"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const platform = require("../lib/platform");

test("maps supported platforms to release assets", () => {
  assert.deepEqual(platform.targetFor("linux", "x64"), {
    asset: "soma-linux-x86_64.tar.gz",
    binary: "soma",
  });
  assert.deepEqual(platform.targetFor("win32", "x64"), {
    asset: "soma-windows-x86_64.tar.gz",
    binary: "soma.exe",
  });
});

test("rejects unsupported platforms", () => {
  assert.throws(() => platform.targetFor("darwin", "arm64"), /Unsupported platform/);
});

test("normalizes release versions and download URLs", () => {
  const env = {
    SOMA_RMCP_BINARY_VERSION: "0.8.1",
    SOMA_RMCP_REPO: "dinglebear-ai/soma",
  };
  assert.equal(platform.releaseVersion(env), "v0.8.1");
  assert.equal(
    platform.downloadUrl(platform.targetFor("linux", "x64"), env),
    "https://github.com/dinglebear-ai/soma/releases/download/v0.8.1/soma-linux-x86_64.tar.gz",
  );
  assert.equal(
    platform.checksumUrl(env),
    "https://github.com/dinglebear-ai/soma/releases/download/v0.8.1/SHA256SUMS",
  );
});
