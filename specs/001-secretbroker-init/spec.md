# SecretBroker Initial Product Specification

Status: Accepted; implementation in progress  
Created: 2026-07-21  
Feature: `001-secretbroker-init`

## Summary

SecretBroker gives local coding-agent commands access to environment secrets without requiring users to paste secret values into a model conversation. It is a local-first CLI, secure credential broker, browser/terminal secret-entry flow, and portable Agent Skill with optional native agent adapters.

SecretBroker separates responsibilities:

- The CLI and local broker collect, store, and inject secrets.
- The Agent Skill teaches agents to request and use secrets safely.
- Native adapters improve input interception and command wrapping where an agent exposes extension APIs.

## Goals

1. Keep secret values out of prompts, model context, session transcripts, command arguments, ordinary logs, and tool results.
2. Work from any local coding agent capable of running a command.
3. Offer three collection experiences:
   - secure masked terminal input;
   - one-time localhost browser form;
   - fulfillment from another terminal.
4. Let an agent block until requested secrets are supplied, then continue automatically.
5. Store persistent values in the operating system credential store.
6. Support user, project, and session scopes with optional expiry.
7. Inject only explicitly requested variables into a child process.
8. Redact exact secret values from captured child stdout and stderr.
9. Install one portable skill for Pi, Claude Code, Codex, or all supported agents.
10. Ship as a native Rust binary and as an `npx`-runnable npm package.

## Non-goals for the initial release

- A hosted secret service.
- Sharing secrets between computers or users.
- Enterprise RBAC, audit export, or centralized policy management.
- Defending against a fully compromised local account, kernel, browser, or credential store.
- Guaranteeing that an arbitrary child executable cannot exfiltrate a granted secret.
- Reliably redacting transformed, encoded, hashed, truncated, or encrypted forms of a secret.
- Transparent command interception in every agent without a native adapter.

## Users and use cases

### Local coding-agent user

A user asks an agent to deploy through Vercel. The agent determines that `VERCEL_TOKEN` is unavailable and runs:

```sh
secretbroker request --scope project --var VERCEL_TOKEN --wait
```

SecretBroker asks whether to use the browser, this terminal, or another terminal. The user supplies the value outside the conversation. The request exits successfully and returns only the variable name and readiness state. The agent continues with:

```sh
secretbroker run --scope project --with VERCEL_TOKEN -- vercel deploy
```

### Multiple required values

An agent can request all known missing values in a single form:

```sh
secretbroker request \
  --scope project \
  --var VERCEL_TOKEN="Vercel deployment token" \
  --var SENTRY_AUTH_TOKEN="Sentry release token" \
  --wait
```

Descriptions are non-secret metadata and may be shown in tool output.

### Another terminal

The original command creates a pending request and waits. The user is shown a command such as:

```sh
secretbroker fulfill sbreq_01J...
```

The request ID is an identifier, not permission to retrieve stored values. The fulfillment process only writes values requested by the pending request.

## Functional requirements

### FR-1: Request secrets

`secretbroker request` MUST:

- require one or more valid environment-variable names;
- accept an optional non-secret label/description per variable;
- determine a scope;
- reject duplicates and malformed names;
- skip variables already present and unexpired unless `--replace` is used;
- support `--terminal`, `--web`, `--another-terminal`, and interactive mode selection;
- support `--wait` and a bounded timeout;
- print no secret value;
- produce machine-readable JSON with `--json`.

### FR-2: Secure terminal entry

Terminal entry MUST:

- require a real terminal;
- disable terminal echo;
- support cancellation without saving partial input;
- reject empty values unless explicitly allowed;
- avoid command-line arguments and environment transport for values.

### FR-3: Local browser entry

Browser entry MUST:

- bind to loopback only on an operating-system-selected port;
- use a cryptographically random, one-time capability;
- avoid printing the capability to stdout/stderr;
- keep the capability out of HTTP request paths and query strings where practical;
- serve all assets locally with no third-party dependencies;
- use restrictive security and cache headers;
- reject invalid Host, Origin, method, content type, capability, and request state;
- accept one successful submission;
- expire and shut down automatically;
- never persist values in browser storage or cookies;
- store all submitted values atomically or store none.

### FR-4: Fulfillment in another terminal

Pending requests MUST:

- be represented by non-secret metadata in a user-private runtime directory;
- use files/directories inaccessible to other users where the OS supports permissions;
- expire automatically;
- never contain secret values;
- support a separate `secretbroker fulfill <request-id>` process;
- allow the waiting request process to detect readiness without reading values.

### FR-5: Secret storage

Persistent storage MUST:

- use the OS credential store through a maintained cross-platform abstraction;
- store only metadata such as names, scope IDs, creation times, and expiry times outside that store;
- use atomic metadata writes and restrictive file permissions;
- never provide a `show`, `get`, `export`, or plaintext dump command.

The initial supported credential stores are:

- macOS Keychain;
- Windows Credential Manager;
- Linux Secret Service/keyring where available.

### FR-6: Scopes

Supported scopes:

- `user`: available from any directory for the current OS user;
- `project`: tied to the canonical project directory, represented by a stable hash;
- `session:<id>`: explicit session namespace suitable for short-lived values.

Scope precedence is session, project, then user when resolution is explicitly enabled. Initial `run` calls MUST name every injected variable with `--with`; no implicit injection of all known values is allowed.

### FR-7: Run commands

`secretbroker run` MUST:

- require at least one `--with NAME`;
- resolve only named, available, unexpired values;
- launch the command without putting values in argv;
- inherit the current environment, overriding only named variables;
- capture stdout/stderr by default and redact exact secret byte sequences before forwarding;
- preserve the child exit code and common termination behavior;
- avoid logging the effective environment;
- refuse shell command strings unless the shell itself is the explicitly selected executable.

### FR-8: Status and removal

The CLI MUST support:

- `status`: names and metadata only;
- `unset NAME`: delete one value and metadata;
- `clear`: delete a selected scope after confirmation;
- automatic cleanup of expired metadata and best-effort deletion of expired credentials.

### FR-9: Skill installation

`secretbroker init --agent <pi|claude|codex|all>` MUST install the portable skill into supported user or project locations. Installation MUST:

- never overwrite modified files without confirmation or `--force`;
- install no secrets or local scope metadata;
- explain which locations were changed;
- support uninstalling files it owns.

### FR-10: Agent behavior

The skill MUST instruct agents to:

- never request that a user paste a secret into chat;
- request all known missing variable names together;
- use the blocking request flow when possible;
- use `secretbroker run --with ...` for credentialed commands;
- never run `env`, `printenv`, tracing, or credential display commands under SecretBroker;
- never claim SecretBroker prevents a granted executable from exfiltrating credentials;
- report only names and readiness.

### FR-11: Local MCP desktop integration

`secretbroker mcp` MUST expose a local stdio MCP server that:

- accepts only variable names and bounded, non-sensitive request options;
- launches the existing browser collector in a separate process;
- returns names and readiness metadata only;
- never returns secret values or capability-bearing localhost URLs;
- serves a widget with no credential input controls;
- keeps widget polling tools app-only when supported by the host;
- writes JSON-RPC only to stdout.

## CLI surface for v0.1

```text
secretbroker request [--scope SCOPE] --var NAME[=DESCRIPTION]... [MODE] [--wait]
secretbroker fulfill REQUEST_ID [--terminal|--web]
secretbroker wait REQUEST_ID [--timeout DURATION] [--json]
secretbroker run [--scope SCOPE] --with NAME... -- COMMAND [ARGS...]
secretbroker status [--scope SCOPE] [--json]
secretbroker unset [--scope SCOPE] NAME...
secretbroker clear [--scope SCOPE] [--yes]
secretbroker init --agent AGENT [--global|--project] [--force]
secretbroker mcp
secretbroker completions SHELL
secretbroker doctor [--json]
```

Interactive request mode options:

1. Open a secure browser form.
2. Enter values in this terminal.
3. Enter values from another terminal.
4. Cancel.

Non-interactive execution without an explicit mode MUST fail safely rather than guessing.

## Output contract

Human and JSON output MUST contain only:

- request IDs;
- variable names;
- descriptions;
- scope labels and non-reversible IDs;
- status and expiry;
- child output after best-effort redaction;
- actionable errors without credential values.

The JSON contract MUST remain stable within a major version.

## Threat model

### Protected against

- accidental inclusion of values in user prompts;
- values appearing in CLI argv or normal SecretBroker output;
- plaintext metadata files;
- accidental exact-value echo from a child process;
- remote network access to the browser form;
- stale browser forms after completion or timeout;
- another local OS user reading request metadata under normal permission enforcement;
- MCP widgets or model context receiving values when the documented desktop workflow is followed.

### Not protected against

- malicious or compromised processes running as the same user;
- a malicious coding agent intentionally invoking an allowed executable to exfiltrate a value;
- browser extensions with permission to inspect localhost pages;
- compromised package registries or an unpinned `npx` package;
- transformed secret output that does not match exact redaction patterns;
- secrets exposed by third-party command configuration, plugins, hooks, or debug logs;
- a user or agent deliberately placing a secret in a model-visible MCP argument.

## Security requirements

1. Rust `unsafe` is forbidden in first-party code unless separately reviewed and documented.
2. Dependencies are minimized, pinned by `Cargo.lock`, audited, and covered by supply-chain tooling.
3. Secret-bearing types use explicit wrappers and zeroization where practical.
4. Secret values MUST NOT implement `Debug`, `Display`, serialization, or cloning without review.
5. No telemetry, analytics, crash upload, or network call is permitted.
6. Browser HTML MUST include a strict CSP, `Cache-Control: no-store`, `Referrer-Policy: no-referrer`, `X-Content-Type-Options: nosniff`, and frame denial.
7. Npm examples MUST pin an exact version for unattended use.
8. Errors crossing a secret boundary MUST be reviewed for accidental value inclusion.

## Acceptance criteria for the initial implementation

- A user can collect one or multiple values through masked terminal input.
- A user can collect values through a one-time loopback browser page.
- A waiting request can be fulfilled from another terminal.
- Values survive processes through the native credential store.
- A named value can be injected into a child process without appearing in argv.
- Exact values printed by a child are replaced with `[REDACTED]`.
- Status and JSON output never contain values.
- The generic skill installs for Pi, Claude Code, and Codex.
- The local MCP server exposes metadata-only request and status tools plus a widget without credential fields.
- Unit tests cover validation, scope IDs, metadata, expiry, request state, and redaction.
- Integration tests cover request/fulfill and child injection without recording values.
- `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test` pass.

## Future work

- Signed executable/argument policies and per-command grants.
- Native Pi, Claude Code, and Codex adapters.
- Short-lived cloud credential exchange instead of static tokens.
- Local desktop companion and browser extension for remote agents.
- Hardware-backed keys and biometric approval.
- Enterprise policy and audit integrations containing metadata only.
