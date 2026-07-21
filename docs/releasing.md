# Releasing SecretBroker

Releases are tag-driven and publish the same version to GitHub Releases, crates.io, and npm.

## One-time repository setup

Create a protected GitHub environment named `release` and add:

- `CARGO_REGISTRY_TOKEN`: crates.io API token limited to the `secretbroker` crate when supported.
- `NPM_TOKEN`: npm automation or granular access token limited to the SecretBroker packages.

Configure npm trusted publishing for the release workflow when available. Require an environment reviewer before package publication.

The release workflow needs `id-token: write` only for npm provenance and `contents: write` only for GitHub release creation.

## Release checklist

1. Ensure CI is green on `main`.
2. Review the threat model and security-sensitive dependency changes.
3. Update versions in `Cargo.toml` and `packages/npm/package.json`.
4. Regenerate `Cargo.lock` and `packages/npm/package-lock.json`.
5. Move relevant changelog entries from Unreleased to the release version.
6. Run:

   ```sh
   cargo fmt --all --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all-features --locked
   cargo deny check
   npm ci --prefix packages/npm
   npm test --prefix packages/npm
   cargo package --locked
   ```

7. Commit the release changes.
8. Create and push a signed tag:

   ```sh
   git tag -s v0.1.0 -m "SecretBroker 0.1.0"
   git push origin main v0.1.0
   ```

9. Approve the protected `release` environment after verifying GitHub artifacts and checksums.
10. Verify npm provenance, crates.io metadata, and installation on each supported platform.

## Published npm packages

The launcher package is `secretbroker`. Native packages are implementation details:

- `secretbroker-darwin-arm64`
- `secretbroker-darwin-x64`
- `secretbroker-linux-arm64`
- `secretbroker-linux-x64`
- `secretbroker-win32-x64`

All packages must be published at the same version. Native packages are published before the launcher.

## Rollback

Package registries generally do not permit replacing a published version. If a release is defective:

1. Mark the GitHub release as affected.
2. Deprecate the npm version with an actionable message.
3. Yank the crates.io version only when necessary to prevent new dependency resolution.
4. Rotate any release credential suspected of exposure.
5. Publish a corrected patch version.
