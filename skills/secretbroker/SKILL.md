---
name: secretbroker
description: Securely request environment variables or credentials for local CLI commands without asking the user to paste secret values into the conversation. Use when a command needs an API key, token, password, credential path, or other sensitive environment variable.
---

# SecretBroker

SecretBroker gives a child command access to explicitly named credentials while keeping values outside the conversation, command arguments, and ordinary tool output.

## Mandatory rules

1. Never ask the user to paste, type, or reveal a secret value in chat.
2. Never put a secret value in a command, command argument, generated file, message, log, or tool result.
3. Request all known missing variable names together.
4. Pass only variable names and non-sensitive descriptions to SecretBroker.
5. Use `secretbroker run --with NAME` to run commands that need stored variables.
6. Never execute `env`, `printenv`, shell tracing, credential-display commands, or equivalent inspection under SecretBroker.
7. Never claim that output redaction or SecretBroker prevents a granted executable from deliberately exfiltrating credentials.

## Workflow

Read [references/workflow.md](references/workflow.md), then follow its local executable selection and request workflow.

Before using credentials, read [references/security.md](references/security.md) and apply its command-boundary rules.

## Expected response behavior

Tell the user which variable names are needed and why, never their values. After the request completes, report only readiness by name and continue with the intended command through SecretBroker.
