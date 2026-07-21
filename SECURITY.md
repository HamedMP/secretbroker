# Security Policy

SecretBroker handles credential material and should be treated as security-sensitive pre-release software.

## Reporting a vulnerability

Do not open a public issue containing exploit details or credential material. Until a private reporting address is published, contact the repository owner privately and include only synthetic test values.

Never send real secrets as part of a report.

## Supported versions

No version is currently supported for production use. A supported-version table and response targets will be published with the first public release.

## Security properties

SecretBroker is designed to keep values out of:

- AI prompts and session transcripts when the documented workflow is followed;
- command-line arguments;
- SecretBroker metadata and normal output;
- exact child-process output through best-effort redaction.

Persistent values are stored in the operating system credential store.

## Explicit limitations

SecretBroker does not protect against:

- malicious processes running as the same OS user;
- compromised browsers, browser extensions, operating systems, or credential stores;
- a granted executable deliberately exfiltrating a credential;
- transformed or encoded secret output;
- compromised npm or crate distribution channels;
- repository-controlled command hooks or plugins executed by a granted tool.

Review [`specs/001-secretbroker-init/spec.md`](specs/001-secretbroker-init/spec.md) before evaluating or deploying SecretBroker.
