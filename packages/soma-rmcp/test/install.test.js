"use strict";

const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const test = require("node:test");
const installer = require("../scripts/install");
const platform = require("../lib/platform");

test("parses bounded installer controls", () => {
  assert.equal(installer.parseControl(undefined, 5, "VALUE", { minimum: 1 }), 5);
  assert.equal(installer.parseControl("7", 5, "VALUE", { minimum: 1 }), 7);
  assert.throws(() => installer.parseControl("0", 5, "VALUE", { minimum: 1 }), /positive integer/);
  assert.throws(() => installer.parseControl("1.5", 5, "VALUE", { minimum: 1 }), /positive integer/);
});

test("rejects HTTPS downgrade redirects", () => {
  assert.throws(
    () => installer.resolveRedirect("https://example.test/file", "http://example.test/file"),
    /HTTPS downgrade/,
  );
});

test("reads exact checksums from shared manifest", () => {
  const digest = "a".repeat(64);
  assert.equal(installer.expectedChecksum(digest + "  soma-linux-x86_64.tar.gz", "soma-linux-x86_64.tar.gz"), digest);
  assert.throws(() => installer.expectedChecksum(digest + "  other.tar.gz", "soma-linux-x86_64.tar.gz"), /does not contain/);
});

test("extracts exactly one expected regular binary", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "soma-install-test-"));
  try {
    const source = path.join(root, "source");
    const output = path.join(root, "output");
    const archive = path.join(root, "soma.tar.gz");
    fs.mkdirSync(source);
    fs.writeFileSync(path.join(source, "soma"), "binary");
    const packed = spawnSync("tar", ["-C", source, "-czf", archive, "soma"], { encoding: "utf8" });
    assert.equal(packed.status, 0, packed.stderr);
    const extracted = installer.extractBinary(archive, output, "soma");
    assert.equal(fs.readFileSync(extracted, "utf8"), "binary");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("verifies SHA256SUMS and installs atomically", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "soma-checksum-test-"));
  try {
    const source = path.join(root, "source");
    const destination = path.join(root, "vendor", "soma");
    const checksums = path.join(root, "SHA256SUMS");
    fs.writeFileSync(source, "verified-binary");
    const digest = crypto.createHash("sha256").update("verified-binary").digest("hex");
    fs.writeFileSync(checksums, digest + "  soma-linux-x86_64.tar.gz");
    installer.verifyChecksum(source, checksums, "soma-linux-x86_64.tar.gz");
    installer.atomicInstall(source, destination);
    assert.equal(fs.readFileSync(destination, "utf8"), "verified-binary");
    assert.equal(fs.statSync(destination).mode & 0o777, 0o755);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("downloads, verifies, and installs the native binary", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "soma-download-test-"));
  const source = path.join(root, "source");
  const archive = path.join(root, "soma-linux-x86_64.tar.gz");
  const fakeBin = path.join(root, "bin");
  fs.mkdirSync(source);
  fs.mkdirSync(fakeBin);
  fs.writeFileSync(path.join(source, "soma"), "downloaded-soma-binary");
  const packed = spawnSync("tar", ["-C", source, "-czf", archive, "soma"], { encoding: "utf8" });
  assert.equal(packed.status, 0, packed.stderr);
  const digest = crypto.createHash("sha256").update(fs.readFileSync(archive)).digest("hex");
  const ghScript = [
    "#!/bin/sh",
    'if [ "$1" = "--version" ]; then',
    "  printf '%s' 'gh version 2.68.1'",
    "  exit 0",
    "fi",
    "exit 0",
  ].join(String.fromCharCode(10));
  const ghPath = path.join(fakeBin, "gh");
  fs.writeFileSync(ghPath, ghScript, { mode: 0o755 });

  const server = http.createServer((request, response) => {
    if (request.url.endsWith("/soma-linux-x86_64.tar.gz")) {
      response.writeHead(200, { "Content-Type": "application/gzip" });
      fs.createReadStream(archive).pipe(response);
      return;
    }
    if (request.url.endsWith("/SHA256SUMS")) {
      response.writeHead(200, { "Content-Type": "text/plain" });
      response.end(digest + "  soma-linux-x86_64.tar.gz");
      return;
    }
    response.writeHead(404);
    response.end();
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  const originalPath = process.env.PATH;
  const originalBase = process.env.SOMA_RMCP_RELEASE_BASE_URL;
  const originalVersion = process.env.SOMA_RMCP_BINARY_VERSION;
  fs.rmSync(platform.installRoot(), { recursive: true, force: true });
  try {
    process.env.PATH = fakeBin + path.delimiter + originalPath;
    process.env.SOMA_RMCP_RELEASE_BASE_URL = `http://127.0.0.1:${address.port}`;
    process.env.SOMA_RMCP_BINARY_VERSION = "0.8.1";
    await installer.main();
    assert.equal(fs.readFileSync(platform.binaryPath(), "utf8"), "downloaded-soma-binary");
    assert.equal(fs.statSync(platform.binaryPath()).mode & 0o777, 0o755);
  } finally {
    process.env.PATH = originalPath;
    if (originalBase === undefined) delete process.env.SOMA_RMCP_RELEASE_BASE_URL;
    else process.env.SOMA_RMCP_RELEASE_BASE_URL = originalBase;
    if (originalVersion === undefined) delete process.env.SOMA_RMCP_BINARY_VERSION;
    else process.env.SOMA_RMCP_BINARY_VERSION = originalVersion;
    fs.rmSync(platform.installRoot(), { recursive: true, force: true });
    await new Promise((resolve) => server.close(resolve));
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("requires GitHub CLI 2.68 or newer", () => {
  const good = () => ({ status: 0, stdout: "gh version 2.68.1" });
  const old = () => ({ status: 0, stdout: "gh version 2.67.0" });
  assert.doesNotThrow(() => installer.requireGhVersion(good));
  assert.throws(() => installer.requireGhVersion(old), /2.68/);
});

test("pins provenance verification to repository workflow and tag", () => {
  let args;
  const runner = (_command, received) => {
    args = received;
    return { status: 0 };
  };
  installer.verifyAttestation("archive.tar.gz", "dinglebear-ai/soma", "v0.8.1", runner);
  assert.deepEqual(args, [
    "attestation", "verify", "archive.tar.gz",
    "--repo", "dinglebear-ai/soma",
    "--signer-workflow", "dinglebear-ai/soma/.github/workflows/release.yml",
    "--source-ref", "refs/tags/v0.8.1",
    "--deny-self-hosted-runners",
  ]);
});
