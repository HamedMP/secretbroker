#!/usr/bin/env node

import { chmod, cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";

function parseArguments(values) {
  const result = {};
  for (let index = 0; index < values.length; index += 2) {
    const key = values[index]?.replace(/^--/, "");
    const value = values[index + 1];
    if (!key || !value) throw new Error("Expected --key value arguments");
    result[key] = value;
  }
  return result;
}

const args = parseArguments(process.argv.slice(2));
for (const required of ["binary", "platform", "arch", "version", "out"]) {
  if (!args[required]) throw new Error(`Missing --${required}`);
}

const packageName = `secretbroker-${args.platform}-${args.arch}`;
const destination = resolve(args.out, packageName);
const binaryName = args.platform === "win32" ? "secretbroker.exe" : "secretbroker";

await rm(destination, { recursive: true, force: true });
await mkdir(join(destination, "bin"), { recursive: true });
await cp(resolve(args.binary), join(destination, "bin", binaryName));
if (args.platform !== "win32") await chmod(join(destination, "bin", binaryName), 0o755);
await cp(new URL("../LICENSE", import.meta.url), join(destination, "LICENSE"));

const packageJson = {
  name: packageName,
  version: args.version,
  description: `SecretBroker native binary for ${args.platform}-${args.arch}`,
  os: [args.platform],
  cpu: [args.arch],
  files: ["bin", "LICENSE", "README.md"],
  bin: { secretbroker: `bin/${binaryName}` },
  repository: {
    type: "git",
    url: "git+https://github.com/HamedMP/secretbroker.git",
  },
  homepage: "https://github.com/HamedMP/secretbroker#readme",
  license: "MIT",
};
await writeFile(join(destination, "package.json"), `${JSON.stringify(packageJson, null, 2)}\n`);
await writeFile(
  join(destination, "README.md"),
  `# ${packageName}\n\nPlatform binary used by the [SecretBroker](https://github.com/HamedMP/secretbroker) npm launcher. Install \`secretbroker\` instead of depending on this package directly.\n`,
);

const source = await readFile(resolve(args.binary));
if (source.length === 0) throw new Error(`Staged binary ${basename(args.binary)} is empty`);
process.stdout.write(`${destination}\n`);
