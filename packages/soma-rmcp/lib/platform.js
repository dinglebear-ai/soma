"use strict";

const path = require("node:path");

function packageVersion() {
  return require("../package.json").version;
}

function binaryVersion() {
  return require("../package.json").binaryVersion || packageVersion();
}

function targetFor(platform = process.platform, arch = process.arch) {
  if (platform === "linux" && arch === "x64") {
    return { asset: "soma-linux-x86_64.tar.gz", binary: "soma" };
  }
  if (platform === "win32" && arch === "x64") {
    return { asset: "soma-windows-x86_64.tar.gz", binary: "soma.exe" };
  }
  throw new Error(`Unsupported platform ${platform}/${arch}. Supported targets: linux/x64, win32/x64.`);
}

function releaseVersion(env = process.env) {
  const raw = env.SOMA_RMCP_BINARY_VERSION || env.SOMA_BINARY_VERSION || binaryVersion();
  return raw.startsWith("v") ? raw : `v${raw}`;
}

function releaseBaseUrl(env = process.env) {
  const repo = env.SOMA_RMCP_REPO || "dinglebear-ai/soma";
  return env.SOMA_RMCP_RELEASE_BASE_URL || `https://github.com/${repo}/releases/download`;
}

function downloadUrl(target, env = process.env) {
  return `${releaseBaseUrl(env)}/${releaseVersion(env)}/${target.asset}`;
}

function checksumUrl(env = process.env) {
  return `${releaseBaseUrl(env)}/${releaseVersion(env)}/SHA256SUMS`;
}

function installRoot() {
  return path.resolve(__dirname, "..", "vendor");
}

function binaryPath(platform = process.platform, arch = process.arch) {
  return path.join(installRoot(), targetFor(platform, arch).binary);
}

module.exports = {
  binaryPath,
  binaryVersion,
  checksumUrl,
  downloadUrl,
  installRoot,
  packageVersion,
  releaseBaseUrl,
  releaseVersion,
  targetFor,
};
