<p align="center">
  <img src="assets/secretbroker-banner.svg" alt="SecretBroker — keep secrets out of agent context" width="100%">
</p>

<p align="center">
  <a href="https://github.com/HamedMP/secretbroker/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/HamedMP/secretbroker/ci.yml?branch=main&amp;style=flat-square&amp;label=CI" alt="CI status"></a>
  <a href="https://crates.io/crates/secretbroker"><img src="https://img.shields.io/crates/v/secretbroker?style=flat-square" alt="crates.io version"></a>
  <a href="https://www.npmjs.com/package/secretbroker"><img src="https://img.shields.io/npm/v/secretbroker?style=flat-square" alt="npm version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-818cf8?style=flat-square" alt="MIT license"></a>
  <img src="https://img.shields.io/badge/Rust-1.88%2B-38bdf8?style=flat-square" alt="Rust 1.88 or newer">
  <img src="https://img.shields.io/badge/telemetry-none-c084fc?style=flat-square" alt="No telemetry">
</p>

<p align="center"><strong>Secure local input. Scoped command access. Native keychains.</strong></p>

SecretBroker gives local commands access to explicitly named secrets without asking users to paste secret values into an AI conversation.

It combines:

- a native Rust CLI;
- masked terminal collection;
- a one-time localhost browser form;
- fulfillment from another terminal;
- OS credential-store persistence;
- exact-value output redaction;
- a portable Agent Skill for Pi, Claude Code, and Codex.

> SecretBroker is pre-release software. Do not use it with production credentials until the threat model and platform integration have been independently reviewed.

## Why

Coding agents often need credentials for commands such as `vercel`, `gcloud`, `aws`, or `sentry-cli`. Pasting those values into chat places them in model context and usually in a session transcript. SecretBroker moves collection outside the conversation and injects values only into the selected child process.

## Build

Rust 1.88 or newer is required.

```sh
cargo build
cargo test
```

## Basic workflow

Ask for one or more values through a secure local UI:

```sh
secretbroker request \
  --scope project \
  --var VERCEL_TOKEN="Vercel deployment token" \
  --web
```

Or use masked terminal input:

```sh
secretbroker request --scope project --var VERCEL_TOKEN --terminal
```

For another terminal:

```sh
secretbroker request \
  --scope project \
  --var VERCEL_TOKEN \
  --another-terminal \
  --wait
```

Run a command with only the named value:

```sh
secretbroker run \
  --scope project \
  --with VERCEL_TOKEN \
  -- vercel deploy
```

Inspect metadata without retrieving values:

```sh
secretbroker status --scope project --json
```

SecretBroker intentionally has no `get`, `show`, or `export` command.

## Install the Agent Skill

Project-local installation for every supported agent:

```sh
secretbroker init --agent all
```

Global installation:

```sh
secretbroker init --agent all --global
```

The skill tells agents never to request values in chat and to use the explicit request/run workflow.

## `npx` distribution

The npm launcher lives in [`packages/npm`](packages/npm). Public releases will bundle signed platform binaries so this works without a Rust toolchain:

```sh
npx -y secretbroker@0.1.0 request --scope project --var VERCEL_TOKEN --web
```

Always pin an exact reviewed version when downloading credential-handling software.

## Security boundary

SecretBroker reduces accidental disclosure. It is not an adversarial sandbox. A process that receives a secret can deliberately print, transform, or transmit it. Exact output redaction cannot catch encoded or transformed values.

Use short-lived, least-privilege credentials and invoke the intended executable directly. Read the full specification and threat model in [`specs/001-secretbroker-init/spec.md`](specs/001-secretbroker-init/spec.md).

## Development status

The active implementation plan and task list are under [`specs/001-secretbroker-init`](specs/001-secretbroker-init/). Release operations are documented in [`docs/releasing.md`](docs/releasing.md).

## License

Licensed under the MIT License.
