# Codex and ChatGPT Desktop Plugin

SecretBroker includes a skills plugin, a local stdio MCP server, and an MCP App status widget. The integration opens SecretBroker's existing one-time loopback form in the system browser; it does not collect credentials inside ChatGPT.

## Install and run

The plugin marketplace installs `.mcp.json`, which starts:

```sh
secretbroker mcp
```

The `secretbroker` binary must already be available on `PATH`. The MCP server exposes:

- `secretbroker_open`: accepts variable names, non-sensitive descriptions, scope, timeout, and optional TTL; launches a separate SecretBroker browser-collection process.
- `secretbroker_status`: app-only polling tool that returns available, missing, and expired names from local metadata.
- `ui://secretbroker/request-status.html`: sandboxed status and retry widget.

## Supported presentation

ChatGPT app components render in a sandboxed iframe within the conversation. They may request inline, picture-in-picture, fullscreen, or host-controlled modal presentation. The current app API does not provide a permanent browser sidebar. The widget can request picture-in-picture so readiness remains visible while the user completes the external form.

## Security-preserving architecture

1. The model calls `secretbroker_open` with names and non-sensitive descriptions only.
2. The MCP server validates and bounds every argument.
3. It starts a separate `secretbroker request --web` child with stdin, stdout, and stderr detached from MCP transport.
4. That child binds the one-time form to an ephemeral loopback port and opens it in the system browser.
5. Neither the capability-bearing localhost URL nor any credential value is returned to the MCP process, widget, model, or transcript.
6. The user enters values only in SecretBroker's local form.
7. The app-only status tool reads metadata, never credential values.
8. Commands still receive credentials only through `secretbroker run --with NAME`.

The widget contains no text or password input. Its tool calls contain only names, scope, and safe request options.

## Hard boundary

The integration must never:

- accept secret values in a ChatGPT component field;
- pass secret values to `window.openai.callTool`;
- return capability URLs in `content`, `structuredContent`, or `_meta`;
- embed the loopback form in a subframe;
- return credential values from MCP tools;
- write MCP diagnostics to stdout, which is reserved for JSON-RPC;
- claim that a child process cannot exfiltrate granted credentials.

## Remaining desktop release work

- Validate the widget visually in ChatGPT developer mode and each supported Codex desktop release.
- Create a developer-mode app ID and `.app.json` only if the target ChatGPT distribution flow requires one; never commit an account-specific placeholder.
- Add screenshots and host-specific accessibility tests.
- Complete an independent security review before submitting the app through the public plugin directory.

## References

- [Build plugins](https://learn.chatgpt.com/docs/build-plugins)
- [Build skills](https://learn.chatgpt.com/docs/build-skills)
- [Build a ChatGPT UI](https://developers.openai.com/apps-sdk/build/chatgpt-ui/)
- [Apps SDK reference](https://developers.openai.com/apps-sdk/reference/)
