# Privacy

SecretBroker is local-first software. The SecretBroker project does not operate a hosted credential service and the CLI contains no telemetry or analytics.

## Local data

Secret values are entered through a masked terminal prompt or a one-time form bound to the local loopback interface. Persistent values are stored in the operating system credential store. Metadata that does not contain secret values is stored locally with restrictive filesystem permissions.

## Child commands

SecretBroker injects only explicitly named variables into the selected child process. That process can access those values and may have its own network, logging, and privacy behavior. Review and trust a command before granting credentials to it.

## Package and source hosting

Downloading SecretBroker or visiting its project pages may involve third-party services such as GitHub, npm, crates.io, or skills.sh. Their privacy policies apply to those services.

## Contact

Report privacy or security concerns through the private vulnerability reporting flow at <https://github.com/HamedMP/secretbroker/security>.
