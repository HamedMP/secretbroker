#!/usr/bin/env node

import { access, readFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const read = (path) => readFile(new URL(path, root), "utf8");
const readJson = async (path) => JSON.parse(await read(path));

const skill = await read("skills/secretbroker/SKILL.md");
const frontmatter = skill.match(/^---\n([\s\S]*?)\n---/)?.[1];
if (!frontmatter) throw new Error("SKILL.md frontmatter is missing");
const field = (name) => frontmatter.match(new RegExp(`^${name}:\\s*(.+)$`, "m"))?.[1]?.trim();
const name = field("name");
const description = field("description");
if (name !== "secretbroker") throw new Error("Skill name must match its directory");
if (!/^[a-z0-9-]{1,64}$/.test(name)) throw new Error("Skill name is not valid kebab-case");
if (!description || description.length > 1024) throw new Error("Skill description is missing or too long");
if (field("license") !== "MIT") throw new Error("Skill license must be MIT");

const requiredFiles = [
  ".mcp.json",
  "assets/secretbroker-widget.html",
  "skills/secretbroker/references/workflow.md",
  "skills/secretbroker/references/security.md",
  "skills/secretbroker/agents/openai.yaml",
  "skills/secretbroker/assets/secretbroker-icon.svg",
  "skills/secretbroker/assets/secretbroker-icon.png",
];
await Promise.all(requiredFiles.map((path) => access(new URL(path, root))));

const evals = await readJson("skills/secretbroker/evals/evals.json");
if (evals.length !== 8 || evals.some((entry) => entry.skills?.[0] !== "secretbroker")) {
  throw new Error("Expected eight SecretBroker trigger and safety evaluations");
}

const skillsConfig = await readJson("skills.sh.json");
if (!skillsConfig.groupings?.some((group) => group.skills?.includes("secretbroker"))) {
  throw new Error("skills.sh.json does not expose secretbroker");
}

const claudePlugin = await readJson(".claude-plugin/plugin.json");
const claudeMarketplace = await readJson(".claude-plugin/marketplace.json");
const codexPlugin = await readJson(".codex-plugin/plugin.json");
const codexMarketplace = await readJson(".agents/plugins/marketplace.json");
const mcpConfig = await readJson(".mcp.json");
if (claudePlugin.name !== "secretbroker" || codexPlugin.name !== "secretbroker") {
  throw new Error("Plugin manifests must use the secretbroker identifier");
}
if (!claudeMarketplace.plugins?.some((plugin) => plugin.name === "secretbroker")) {
  throw new Error("Claude marketplace does not expose secretbroker");
}
if (!codexMarketplace.plugins?.some((plugin) => plugin.name === "secretbroker")) {
  throw new Error("Codex marketplace does not expose secretbroker");
}
if (codexPlugin.skills !== "./skills/") throw new Error("Codex plugin skill path is invalid");
if (codexPlugin.mcpServers !== "./.mcp.json") throw new Error("Codex MCP path is invalid");
if (mcpConfig.secretbroker?.command !== "secretbroker" || mcpConfig.secretbroker?.args?.[0] !== "mcp") {
  throw new Error("MCP server command is invalid");
}
const widget = await read("assets/secretbroker-widget.html");
if (/<input|type=["']password/i.test(widget)) {
  throw new Error("Desktop widget must not contain credential input controls");
}

process.stdout.write("Agent skill and plugin packages are valid.\n");
