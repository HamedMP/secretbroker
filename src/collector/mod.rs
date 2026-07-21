pub mod terminal;
pub mod web;

use serde::{Deserialize, Serialize};

use crate::{env_name::EnvName, secret::SecretValue};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VariableRequest {
    pub name: EnvName,
    pub description: Option<String>,
}

pub type CollectedSecret = (EnvName, SecretValue);
