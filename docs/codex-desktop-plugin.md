# Codex and ChatGPT Desktop Plugin

SecretBroker is currently packaged as a skills-only Codex plugin. A future MCP-backed app can add a native control surface in the ChatGPT desktop app without collecting secret values inside ChatGPT.

## Supported presentation

ChatGPT app components render in a sandboxed iframe within the conversation. They may request inline, picture-in-picture, fullscreen, or host-controlled modal presentation. The current app API does not provide a permanent browser sidebar.

The safest useful interface is a small request/status widget that opens SecretBroker's one-time loopback form in the system browser.

## Security-preserving architecture

1. A local SecretBroker MCP bridge exposes a `create_secret_request` tool.
2. Its input schema accepts only variable names, non-sensitive descriptions, scope, and collection mode.
3. The bridge starts the existing one-time loopback collector.
4. Safe request metadata is returned in `structuredContent`.
5. The capability-bearing localhost URL is returned only in tool-result `_meta`, which ChatGPT delivers to the component but hides from the model and conversation transcript.
6. The component uses `window.openai.openExternal` to open that URL in the system browser.
7. The user enters values only in SecretBroker's local form.
8. The widget polls safe request status and reports readiness by variable name.
9. Commands still receive credentials only through `secretbroker run --with NAME`.

The app must never:

- accept secret values in a ChatGPT component field;
- pass secret values to `window.openai.callTool`;
- place the capability URL in `content` or `structuredContent`;
- embed the loopback form in a subframe;
- return credential values from MCP tools;
- claim that the child process is unable to exfiltrate granted credentials.

## Why an external form

Tool inputs, `content`, and `structuredContent` can become model-visible or appear in the conversation transcript. Tool-result `_meta` is widget-only, but using it for a short-lived launch URL is safer than moving credential values through the app bridge. The existing loopback form also preserves SecretBroker's host and origin validation, no-store policy, capability checks, and process lifetime.

## Implementation stages

1. Add a local `secretbroker mcp` transport exposing request, status, and cancel operations only.
2. Build an instruction-light MCP App widget with no secret input controls.
3. Add app-only tool visibility for widget actions that the model should not invoke.
4. Create a developer-mode app in ChatGPT and add its `plugin_asdk_app_...` identifier to `.app.json`.
5. Point `.codex-plugin/plugin.json` at `.app.json` and test through the local marketplace.
6. Add tests proving that values and capability URLs never enter model-visible result fields.
7. Complete independent security review before public submission.

## References

- [Build plugins](https://learn.chatgpt.com/docs/build-plugins)
- [Build skills](https://learn.chatgpt.com/docs/build-skills)
- [Build a ChatGPT UI](https://developers.openai.com/apps-sdk/build/chatgpt-ui/)
- [Apps SDK reference](https://developers.openai.com/apps-sdk/reference/)
