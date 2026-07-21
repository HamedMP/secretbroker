use anyhow::{Result, bail};

use crate::{collector::VariableRequest, secret::SecretValue};

use super::CollectedSecret;

pub fn collect(requests: &[VariableRequest], allow_empty: bool) -> Result<Vec<CollectedSecret>> {
    let mut collected = Vec::with_capacity(requests.len());
    for request in requests {
        let prompt = request.description.as_ref().map_or_else(
            || format!("{}: ", request.name),
            |description| format!("{} ({description}): ", request.name),
        );
        let value = SecretValue::new(rpassword::prompt_password(prompt)?);
        if value.is_empty() && !allow_empty {
            bail!("{} cannot be empty; no values were stored", request.name);
        }
        collected.push((request.name.clone(), value));
    }
    Ok(collected)
}
