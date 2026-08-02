#!/usr/bin/env node
"use strict";

const { spawn } = require("node:child_process");
const path = require("node:path");
const { binaryPath } = require("../lib/platform");

const binary = binaryPath();
const child = spawn(binary, process.argv.slice(2), {
  stdio: "inherit",
  env: process.env,
});

child.on("error", (error) => {
  if (error.code === "ENOENT") {
    console.error(`Unable to find the installed Soma binary at ${path.resolve(binary)}. Reinstall @dinglebear/soma.`);
    process.exit(127);
  }
  console.error(error.message);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
