#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const executableName = process.platform === "win32" ? "secretbroker.exe" : "secretbroker";

function platformPackage() {
  const packages = new Map([
    ["darwin-arm64", "secretbroker-darwin-arm64"],
    ["darwin-x64", "secretbroker-darwin-x64"],
    ["linux-arm64", "secretbroker-linux-arm64"],
    ["linux-x64", "secretbroker-linux-x64"],
    ["win32-x64", "secretbroker-win32-x64"],
  ]);
  const target = `${process.platform}-${process.arch}`;
  const packageName = packages.get(target);
  if (!packageName) {
    throw new Error(`SecretBroker does not provide a binary for ${target}`);
  }
  return packageName;
}

function resolveBinary() {
  const override = process.env.SECRETBROKER_BINARY;
  if (override) {
    const absolute = resolve(override);
    if (!existsSync(absolute)) {
      throw new Error("SECRETBROKER_BINARY points to a missing file");
    }
    return absolute;
  }

  const packageName = platformPackage();
  try {
    const packageJson = require.resolve(`${packageName}/package.json`);
    const binary = join(dirname(packageJson), "bin", executableName);
    if (!existsSync(binary)) {
      throw new Error(`${packageName} is installed but its binary is missing`);
    }
    return binary;
  } catch (error) {
    if (error instanceof Error && error.message.includes("binary is missing")) {
      throw error;
    }

    const developmentBinary = fileURLToPath(
      new URL(`../../../target/debug/${executableName}`, import.meta.url),
    );
    if (existsSync(developmentBinary)) {
      return developmentBinary;
    }
    throw new Error(
      `The optional package ${packageName} is missing. Reinstall secretbroker with optional dependencies enabled.`,
      { cause: error },
    );
  }
}

try {
  const binary = resolveBinary();
  const result = spawnSync(binary, process.argv.slice(2), {
    stdio: "inherit",
    env: process.env,
    windowsHide: false,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.signal) {
    process.kill(process.pid, result.signal);
  }
  process.exit(result.status ?? 1);
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`secretbroker: ${message}\n`);
  process.exit(1);
}
