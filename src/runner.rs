use std::{
    ffi::OsString,
    process::{ExitStatus, Stdio},
    sync::Arc,
};

use anyhow::{Context, Result, bail};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    process::Command,
};
use zeroize::Zeroize;

use crate::{
    env_name::EnvName,
    scope::Scope,
    storage::{CredentialStore, SecretRepository},
};

const STREAM_BUFFER_SIZE: usize = 8 * 1024;

pub async fn run<S: CredentialStore>(
    repository: &SecretRepository<S>,
    scope: &Scope,
    names: &[EnvName],
    command: &[OsString],
) -> Result<ExitStatus> {
    if command.is_empty() {
        bail!("a command is required after --");
    }
    if names.is_empty() {
        bail!("at least one --with variable is required");
    }

    let mut secrets = Vec::with_capacity(names.len());
    for name in names {
        secrets.push((name.clone(), repository.resolve(scope, name)?));
    }
    let patterns = Arc::new(SecretPatterns::new(
        secrets
            .iter()
            .map(|(_, value)| value.expose().as_bytes().to_vec())
            .collect(),
    ));

    let mut child = Command::new(&command[0]);
    child.args(&command[1..]);
    for (name, value) in &secrets {
        child.env(name.as_str(), value.expose());
    }
    child
        .env_remove("SECRETBROKER_HOME")
        .env_remove("SECRETBROKER_BINARY")
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = child
        .spawn()
        .with_context(|| format!("cannot execute {}", command[0].to_string_lossy()))?;
    let stdout = child.stdout.take().context("cannot capture child stdout")?;
    let stderr = child.stderr.take().context("cannot capture child stderr")?;

    let stdout_patterns = Arc::clone(&patterns);
    let stdout_task =
        tokio::spawn(
            async move { redact_stream(stdout, tokio::io::stdout(), stdout_patterns).await },
        );
    let stderr_task =
        tokio::spawn(async move { redact_stream(stderr, tokio::io::stderr(), patterns).await });

    let status = child
        .wait()
        .await
        .context("cannot wait for child command")?;
    stdout_task
        .await
        .context("stdout redaction task failed")??;
    stderr_task
        .await
        .context("stderr redaction task failed")??;
    Ok(status)
}

struct SecretPatterns {
    values: Vec<Vec<u8>>,
    longest: usize,
}

impl SecretPatterns {
    fn new(mut values: Vec<Vec<u8>>) -> Self {
        values.retain(|value| !value.is_empty());
        values.sort_by_key(|right| std::cmp::Reverse(right.len()));
        values.dedup();
        let longest = values.first().map_or(0, Vec::len);
        Self { values, longest }
    }
}

impl Drop for SecretPatterns {
    fn drop(&mut self) {
        for value in &mut self.values {
            value.zeroize();
        }
    }
}

struct StreamingRedactor {
    patterns: Arc<SecretPatterns>,
    pending: Vec<u8>,
}

impl StreamingRedactor {
    fn new(patterns: Arc<SecretPatterns>) -> Self {
        Self {
            patterns,
            pending: Vec::with_capacity(STREAM_BUFFER_SIZE),
        }
    }

    fn push(&mut self, input: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(input);
        self.take_redacted(false)
    }

    fn finish(&mut self) -> Vec<u8> {
        self.take_redacted(true)
    }

    fn take_redacted(&mut self, finish: bool) -> Vec<u8> {
        let retained = self.patterns.longest.saturating_sub(1);
        let limit = if finish {
            self.pending.len()
        } else {
            self.pending.len().saturating_sub(retained)
        };
        let mut output = Vec::with_capacity(limit);
        let mut index = 0;
        while index < limit {
            if let Some(pattern) = self
                .patterns
                .values
                .iter()
                .find(|pattern| self.pending[index..].starts_with(pattern))
            {
                output.extend_from_slice(b"[REDACTED]");
                index += pattern.len();
            } else {
                output.push(self.pending[index]);
                index += 1;
            }
        }
        self.pending.drain(..index);
        output
    }
}

impl Drop for StreamingRedactor {
    fn drop(&mut self) {
        self.pending.zeroize();
    }
}

async fn redact_stream<R, W>(
    mut reader: R,
    mut writer: W,
    patterns: Arc<SecretPatterns>,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut redactor = StreamingRedactor::new(patterns);
    let mut buffer = [0_u8; STREAM_BUFFER_SIZE];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let redacted = redactor.push(&buffer[..count]);
        buffer[..count].zeroize();
        writer.write_all(&redacted).await?;
    }
    writer.write_all(&redactor.finish()).await?;
    writer.flush().await?;
    buffer.zeroize();
    Ok(())
}

#[must_use]
pub fn redact(input: &[u8], patterns: &[&[u8]]) -> Vec<u8> {
    let patterns = Arc::new(SecretPatterns::new(
        patterns.iter().map(|pattern| pattern.to_vec()).collect(),
    ));
    let mut redactor = StreamingRedactor::new(patterns);
    let mut output = redactor.push(input);
    output.extend(redactor.finish());
    output
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{SecretPatterns, StreamingRedactor, redact};

    #[test]
    fn redacts_exact_values() {
        assert_eq!(
            redact(b"before secret after secret", &[b"secret"]),
            b"before [REDACTED] after [REDACTED]"
        );
    }

    #[test]
    fn longest_match_wins() {
        assert_eq!(
            redact(b"token-long", &[b"token", b"token-long"]),
            b"[REDACTED]"
        );
    }

    #[test]
    fn handles_binary_output() {
        assert_eq!(redact(b"\0secret\xff", &[b"secret"]), b"\0[REDACTED]\xff");
    }

    #[test]
    fn ignores_empty_patterns() {
        assert_eq!(redact(b"unchanged", &[b""]), b"unchanged");
    }

    #[test]
    fn redacts_values_split_across_stream_chunks() {
        let patterns = Arc::new(SecretPatterns::new(vec![b"split-secret".to_vec()]));
        let mut redactor = StreamingRedactor::new(patterns);
        let mut output = redactor.push(b"before split-");
        output.extend(redactor.push(b"secret after"));
        output.extend(redactor.finish());
        assert_eq!(output, b"before [REDACTED] after");
    }
}
