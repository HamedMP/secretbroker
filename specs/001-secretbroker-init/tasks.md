# SecretBroker Tasks

Status: In progress

## 0. Specification and decisions

- [x] Write product specification and acceptance criteria.
- [x] Write architecture and delivery plan.
- [x] Select Rust for the security-sensitive native core.
- [x] Confirm unscoped npm names are currently unclaimed.
- [ ] Perform trademark and organization-name checks before publication.

## 1. Repository foundation

- [x] Create Rust package/workspace with strict lints.
- [x] Add README, MIT license, security policy, and contributing guidance.
- [x] Add `.gitignore`, formatting, and dependency-policy configuration.
- [x] Add cross-platform CI for fmt, Clippy, and tests.
- [x] Add `cargo-audit` and dependency/license policy CI.

## 2. Domain and storage foundation

- [x] Implement validated environment variable names.
- [x] Implement `user`, `project`, and `session` scopes.
- [x] Implement secret wrapper with redacted debug behavior and zeroization.
- [x] Define credential-store trait and native keyring implementation.
- [x] Define metadata schema with explicit versioning and expiry.
- [x] Implement private application directories and atomic metadata writes.
- [x] Add in-memory storage backend for tests.

## 3. CLI foundation

- [x] Implement strict Clap command grammar.
- [x] Implement safe human and versioned JSON output.
- [x] Implement `status`.
- [x] Implement `unset`.
- [x] Implement `clear` with confirmation safeguards.
- [x] Implement `doctor`.
- [x] Implement shell completions.

## 4. Terminal collection

- [x] Implement `request --terminal` with no-echo input.
- [x] Collect all requested values before writing any.
- [x] Reject empty values by default.
- [x] Support replace and expiry options.
- [x] Ensure errors and cancellation do not expose or partially save values.

## 5. Secure command execution

- [x] Implement `run --with NAME -- COMMAND...`.
- [x] Resolve only explicitly named variables.
- [x] Reject missing or expired values.
- [x] Inject values only through child environment.
- [x] Capture and redact exact secret bytes from stdout/stderr.
- [x] Preserve exit status and cancellation behavior.
- [x] Add explicit warning/documentation for the arbitrary-command trust boundary.

## 6. Pending requests and another-terminal flow

- [x] Define the pending request schema and state machine.
- [x] Generate cryptographically random request IDs.
- [x] Write request metadata atomically with user-only permissions.
- [x] Implement `request --another-terminal --wait`.
- [x] Implement `fulfill REQUEST_ID`.
- [x] Implement `wait REQUEST_ID` with timeout.
- [x] Add bounded-retention cleanup for expired request bodies.
- [ ] Test coordination across separate processes.

## 7. Browser collection

- [x] Bind an ephemeral loopback-only server.
- [x] Generate a one-time 256-bit browser capability.
- [x] Build embedded, dependency-free HTML/CSS/JavaScript.
- [x] Deliver the capability through a URL fragment without logging it.
- [x] Add Host, Origin, method, content-type, capability, and state validation.
- [x] Add CSP, no-store, no-referrer, no-sniff, and frame-denial headers.
- [x] Store submissions with rollback on metadata failure.
- [x] Shut down after one submission, cancellation, or timeout.
- [x] Add browser collector security tests.

## 8. Generic Agent Skill

- [x] Create `skills/secretbroker/SKILL.md` with precise activation triggers.
- [x] Add command workflow reference.
- [x] Add security boundaries and anti-patterns reference.
- [x] Ensure the skill never directs users to paste values into chat.
- [x] Implement `init --agent pi|claude|codex|all`.
- [x] Support global/project installation and idempotency.
- [ ] Add installer tests for every target layout and overwrite case.

## 9. Npm/npx distribution

- [x] Create the `secretbroker` npm launcher package.
- [ ] Create and publish platform-specific binary packages.
- [x] Fail clearly on unsupported platforms.
- [x] Test the npm launcher against a selected native binary.
- [ ] Add release builds for macOS, Linux, and Windows.
- [ ] Add checksums, signatures, and npm provenance.
- [x] Document version pinning and package supply-chain risks.

## 10. Native adapters

- [ ] Build Pi extension with a native masked popup.
- [ ] Wrap Pi agent Bash and user `!` commands through SecretBroker.
- [ ] Investigate and implement supported Claude Code hooks/plugin UX.
- [ ] Investigate and implement supported Codex integration UX.
- [ ] Ensure all adapters use the same CLI protocol and canonical skill.

## 11. Hardening before public v0.1

- [ ] Complete threat-model review.
- [ ] Complete browser-flow security review.
- [ ] Complete process/executable trust-boundary review.
- [ ] Fuzz parsers and redaction.
- [ ] Test crash recovery and interrupted writes.
- [ ] Verify no telemetry or unexpected network access.
- [x] Run dependency, advisory, and license policy checks.
- [x] Publish documented known limitations.
