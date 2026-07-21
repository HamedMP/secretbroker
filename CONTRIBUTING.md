# Contributing

SecretBroker is security-sensitive. Keep changes narrow, tested, and explicit about trust boundaries.

## Local checks

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Requirements

- Do not add telemetry or hosted network dependencies.
- Do not log, serialize, display, or place secret values in errors.
- Avoid `unsafe`; first-party code forbids it.
- Minimize dependencies and explain security-sensitive additions.
- Add tests for every parser, state transition, filesystem change, and output boundary.
- Use synthetic values in tests and examples.
- Update the active specification and task list when behavior changes.

## Commits

Use concise commit messages focused on why the change is needed. Do not add co-authored-by lines.
