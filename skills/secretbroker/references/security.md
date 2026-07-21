# SecretBroker Security Boundaries

## Protected workflow

SecretBroker is intended to prevent accidental disclosure through prompts, argv, metadata files, normal output, and exact-value echoes from child commands. Persistent values are stored in the operating system credential store.

## Hard boundary

A process that receives an environment variable can attempt to print, transform, transmit, or otherwise exfiltrate it. Exact output redaction is a defense against accidents, not an adversarial sandbox.

Before granting a value:

- use the minimum required variable names;
- prefer short-lived and least-privilege credentials;
- invoke the intended executable directly;
- avoid repository-controlled wrappers, hooks, plugins, and debug modes when they are not trusted;
- do not use shell tracing;
- do not run environment inspection;
- inspect surprising failures without asking for or displaying the value.

## User interaction

The user must enter values only into SecretBroker's masked terminal prompt or one-time localhost form. If the user pastes a value into chat anyway, treat it as compromised and recommend rotation; do not repeat it.

## Package execution

`npx` downloads executable code. Prefer an audited installed binary. If `npx` is required, pin an exact reviewed version. Never suggest an unpinned package in unattended or production workflows.

## Remote agents

A localhost browser flow works only when SecretBroker runs on the user's machine. Do not expose the local form on `0.0.0.0`, tunnel it publicly, or ask the user to submit a secret to an agent-hosted web service.
