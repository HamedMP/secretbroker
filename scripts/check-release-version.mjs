#!/usr/bin/env node

import { readFile } from "node:fs/promises";

const tag = process.argv[2] ?? process.env.GITHUB_REF_NAME;
if (!tag?.startsWith("v")) throw new Error("Expected a v-prefixed release tag");
const expected = tag.slice(1);
const cargoToml = await readFile(new URL("../Cargo.toml", import.meta.url), "utf8");
const cargoVersion = cargoToml.match(/^version = "([^"]+)"$/m)?.[1];
const npmPackage = JSON.parse(await readFile(new URL("../packages/npm/package.json", import.meta.url), "utf8"));
if (cargoVersion !== expected || npmPackage.version !== expected) {
  throw new Error(
    `Tag ${expected}, Cargo ${cargoVersion ?? "missing"}, and npm ${npmPackage.version ?? "missing"} must match`,
  );
}
process.stdout.write(`Release version ${expected} is consistent.\n`);
