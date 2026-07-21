# SecretBroker Implementation Plan

Status: Active  
Specification: `spec.md`

## Technical decision

SecretBroker will be implemented in Rust.

Rust is the best long-term fit because SecretBroker is a local security boundary that needs predictable process control, explicit error handling, memory safety without a garbage collector, cross-platform native credential-store access, and distributable static or near-static binaries. TypeScript will be limited to an npm launcher and optional agent adapters.

## Repository layout

```text
secretbroker/
├── Cargo.toml
├── Cargo.lock
├── crates/
│   ├── secretbroker-cli/
│   └── secretbroker-core/
├── packages/
│   └── npm/                    # npx launcher and platform package metadata
├── skills/
│   └── secretbroker/
│       ├── SKILL.md
│       └── references/
├── adapters/
│   └── pi/                     # native adapter after core CLI
├── .claude-plugin/             # Claude plugin and marketplace manifests
├── .codex-plugin/              # Codex plugin manifest
├── .mcp.json                   # local MCP server registration
├── specs/001-secretbroker-init/
├── tests/
└── .github/workflows/
```

The first implementation may begin as one Rust binary crate while preserving module boundaries. It can be split into workspace crates only when packaging or testing benefits justify it.

## Architecture

### CLI layer

`clap` derives a strict command grammar. Commands return structured domain results that render as either human-safe text or JSON. Secret-bearing domain types cannot be rendered.

### Scope resolver

- User scope uses a constant local-user namespace.
- Project scope canonicalizes the selected directory and hashes it with SHA-256.
- Session scope validates an explicit opaque identifier.
- Filesystem metadata uses only the scope ID and non-secret display metadata.

### Credential store

The `keyring` crate provides the OS store abstraction. Credential usernames are derived from scope ID and variable name. A separate metadata index records names, timestamps, expiry, and storage version but no values.

### Request coordinator

Pending request files live under an OS runtime/cache directory in a `0700` SecretBroker directory. Each request contains:

- version;
- random request ID;
- requested variable names and descriptions;
- scope descriptor;
- created and expiry times;
- state (`pending`, `ready`, `cancelled`, `expired`);
- no secret values.

Updates use write-to-temp, sync, rename, and restrictive permissions. Waiting uses bounded polling initially. A local socket/watch API can replace polling without changing the CLI contract.

### Secret collectors

#### Terminal collector

Uses a no-echo terminal prompt. Values remain in secret wrapper types and are stored only after all fields validate.

#### Browser collector

A Tokio/Axum server binds an ephemeral loopback port. A 256-bit random capability is delivered in the URL fragment. Local JavaScript removes the fragment from visible history, sends the capability in an authorization header, and submits JSON over loopback. The server validates request state and security headers, stores all values, then gracefully shuts down.

The HTML is embedded in the binary and has no remote dependencies.

#### Another-terminal collector

The requester writes a pending request and waits. `fulfill` opens the terminal/browser collector in another process, stores values, and marks the request ready. The waiting process reports readiness without loading the values.

### Command runner

The runner:

1. resolves only variables listed by `--with`;
2. rejects missing or expired credentials;
3. starts the selected executable directly with argument vectors;
4. places secrets only in the child environment;
5. captures stdout and stderr;
6. performs exact byte-sequence redaction before output;
7. propagates the exit status.

A future policy engine will constrain executable paths and arguments. The initial release clearly documents that arbitrary command execution is not an adversarial security boundary.

### Skill distribution

One canonical Agent Skill is stored in `skills/secretbroker`. The installer copies it to:

- Pi: `.agents/skills/secretbroker` (portable location supported by Pi);
- Claude Code: `.claude/skills/secretbroker`;
- Codex: `.agents/skills/secretbroker`;
- global equivalents selected by each platform.

Because Pi and Codex share the portable `.agents/skills` location, installation deduplicates identical targets.

### Desktop MCP integration

`secretbroker mcp` serves a stdio MCP endpoint from the Rust binary. Its request tool validates names and safe options, then starts the existing browser collector in a separate process with all standard streams detached from MCP JSON-RPC. Its app-only status tool reads metadata only. An embedded MCP App resource renders names and readiness without any credential input controls. The collector opens the capability-bearing URL directly in the system browser, so that URL never crosses MCP.

### Npm distribution

The npm package exposes a small launcher that selects a platform-specific optional dependency containing the Rust binary. The intended UX is:

```sh
npx -y secretbroker@0.1.0 request --var VERCEL_TOKEN --wait
```

Release CI builds and signs binaries for macOS, Linux, and Windows. The package does not compile native code during installation.

## Security design reviews

Implementation gates:

1. Secret type/logging review.
2. Filesystem permissions and atomicity review.
3. Browser localhost/CSRF/CSP review.
4. Process environment and output redaction review.
5. Package provenance and release review.

No hosted component is introduced in v0.1.

## Testing strategy

### Unit tests

- environment variable validation;
- scope canonicalization and stable IDs;
- metadata serialization excludes values;
- expiry decisions;
- request state transitions;
- exact redaction including overlapping values;
- CLI parsing and safe rendering.

### Integration tests

- test credential-store backend through dependency injection;
- terminal collection through a test collector;
- browser submission with invalid and valid capabilities;
- two-process request/fulfill coordination;
- child receives a value while argv and result remain clean;
- exact child echo is redacted;
- skill installation and idempotency;
- MCP stdio initialization, tool metadata, widget resource, and metadata-only status.

### CI

- formatting;
- Clippy with warnings denied;
- tests on macOS, Linux, and Windows;
- dependency vulnerability audit;
- license/policy checks;
- npm launcher tests;
- release artifact smoke tests.

## Delivery phases

### Phase 1: Secure CLI foundation

- repository and CI foundations;
- CLI grammar;
- secret-safe domain types;
- scopes, metadata, credential-store abstraction;
- terminal request, status, unset, clear;
- command runner and redaction.

### Phase 2: Request coordination and browser UI

- pending request state machine;
- another-terminal fulfill/wait flow;
- one-time browser collector;
- timeout and cleanup behavior;
- security-focused integration tests.

### Phase 3: Generic agent distribution

- canonical skill and references;
- installer for Pi, Claude Code, and Codex;
- npm launcher and platform packages;
- shell completions and doctor command.

### Phase 4: Native adapters and hardening

- local Codex and ChatGPT desktop MCP bridge and status widget;
- Pi extension with native popup and transparent Bash wrapping;
- investigate equivalent Claude Code and Codex hooks;
- command grant policy;
- signed release automation and documentation.

## Compatibility policy

- Rust MSRV will be declared before the first public release.
- CLI JSON output is versioned.
- Metadata and request files contain explicit schema versions.
- Breaking CLI changes require a major release after v1.
- Credential identifiers remain stable across compatible releases.
