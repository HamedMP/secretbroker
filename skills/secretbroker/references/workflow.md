# SecretBroker Agent Workflow

## 1. Select the executable

Prefer an installed binary:

```sh
secretbroker --version
```

If unavailable and package execution is allowed, use an exact trusted package version:

```sh
npx -y secretbroker@0.1.0 --version
```

Do not silently use `@latest` for software that handles credentials.

In the examples below, replace `secretbroker` with the selected exact command prefix.

## 2. Determine what is missing

Use safe status metadata:

```sh
secretbroker status --scope project --json
```

Do not inspect the process environment. Identify required variable names from official command documentation or a safe error that names a missing variable without printing its value.

## 3. Ask how the user wants to enter values

Unless the user already chose a mode, ask one safe, non-secret question before running the request:

> Should SecretBroker open its secure local browser form, or would you rather enter the values from another terminal?

Do not ask for the values themselves. Use the user's answer to select an explicit mode because agent shell tools often do not provide an interactive terminal.

For the local browser, run:

```sh
secretbroker request \
  --scope project \
  --var VERCEL_TOKEN="Vercel deployment token" \
  --var SENTRY_AUTH_TOKEN="Sentry release token" \
  --web \
  --json
```

For another terminal, first create a detached request so the fulfillment command cannot be hidden by an agent tool that buffers output:

```sh
secretbroker request \
  --scope project \
  --var VERCEL_TOKEN="Vercel deployment token" \
  --var SENTRY_AUTH_TOKEN="Sentry release token" \
  --another-terminal \
  --json
```

Show the returned `secretbroker fulfill <request-id>` command to the user, then wait in a separate call:

```sh
secretbroker wait <request-id> --timeout 10m --json
```

The wait exits when fulfillment succeeds, allowing the agent to continue automatically. A request ID is metadata and never permits reading credential values. A human running SecretBroker directly may use `--another-terminal --wait` as a one-command alternative.

When a human invokes SecretBroker from an interactive terminal without an explicit mode, SecretBroker itself offers browser, current-terminal, another-terminal, and cancel options.

## 4. Run with the minimum variables

Explicitly name only the values needed by this command:

```sh
secretbroker run \
  --scope project \
  --with VERCEL_TOKEN \
  -- vercel deploy
```

Do not wrap a general-purpose shell unless the task truly requires one. Prefer the direct executable and an argument vector.

## 5. Report safely

Allowed:

- `VERCEL_TOKEN is ready.`
- `The deployment completed with exit code 0.`
- `SecretBroker reports that SENTRY_AUTH_TOKEN is missing.`

Forbidden:

- credential values or fragments;
- output from environment-inspection commands;
- URLs containing authentication capabilities;
- claims that a secret cannot be exfiltrated by a command that receives it.
