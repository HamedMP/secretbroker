#!/usr/bin/env node

import { readFile } from "node:fs/promises";

const tag = process.argv[2] ?? process.env.GITHUB_REF_NAME;
if (!tag?.startsWith("v")) throw new Error("Expected a v-prefixed release tag");
const expected = tag.slice(1);
const cargoToml = await readFile(new URL("../Cargo.toml", import.meta.url), "utf8");
const cargoVersion = cargoToml.match(/^version = "([^"]+)"$/m)?.[1];
const readJson = async (path) => JSON.parse(await readFile(new URL(path, import.meta.url), "utf8"));
const npmPackage = await readJson("../packages/npm/package.json");
const claudePlugin = await readJson("../.claude-plugin/plugin.json");
const claudeMarketplace = await readJson("../.claude-plugin/marketplace.json");
const codexPlugin = await readJson("../.codex-plugin/plugin.json");
const versions = {
  Cargo: cargoVersion,
  npm: npmPackage.version,
  "Claude plugin": claudePlugin.version,
  "Claude marketplace": claudeMarketplace.plugins?.[0]?.version,
  "Codex plugin": codexPlugin.version,
};
const mismatches = Object.entries(versions).filter(([, version]) => version !== expected);
if (mismatches.length > 0) {
  const found = mismatches.map(([name, version]) => `${name} ${version ?? "missing"}`).join(", ");
  throw new Error(`Tag ${expected} does not match: ${found}`);
}
process.stdout.write(`Release version ${expected} is consistent.\n`);
