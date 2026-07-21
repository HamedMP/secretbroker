use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    str::FromStr,
    sync::Arc,
};

use anyhow::{Context, Result};
use rmcp::{
    RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, ContentBlock, ErrorData, Implementation, JsonObject,
        ListResourcesResult, ListToolsResult, Meta, PaginatedRequestParams,
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
        ServerCapabilities, ServerInfo, Tool, ToolAnnotations,
    },
    service::RequestContext,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{process::Command, sync::Mutex};

use crate::{
    env_name::EnvName,
    metadata::{MetadataStore, unix_timestamp},
    paths::AppPaths,
    scope::Scope,
    storage::{NativeCredentialStore, SecretRepository},
};

const WIDGET_URI: &str = "ui://secretbroker/request-status.html";
const WIDGET_HTML: &str = include_str!("../assets/secretbroker-widget.html");
const MAX_VARIABLES: usize = 16;
const MAX_DESCRIPTION_BYTES: usize = 256;
const MAX_TIMEOUT_MINUTES: u64 = 60;
const MAX_TTL_MINUTES: u64 = 7 * 24 * 60;

#[derive(Clone, Debug, Default)]
pub struct SecretBrokerMcp {
    launches: Arc<Mutex<HashMap<String, LaunchRecord>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LaunchState {
    Running,
    Failed,
}

#[derive(Clone, Debug)]
struct LaunchRecord {
    generation: String,
    state: LaunchState,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenRequest {
    #[serde(default = "default_scope")]
    scope: String,
    variables: Vec<RequestedVariable>,
    #[serde(default)]
    replace: bool,
    #[serde(default = "default_timeout_minutes")]
    timeout_minutes: u64,
    ttl_minutes: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestedVariable {
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusRequest {
    #[serde(default = "default_scope")]
    scope: String,
    variables: Vec<String>,
}

pub async fn serve() -> Result<()> {
    SecretBrokerMcp::default()
        .serve(rmcp::transport::stdio())
        .await
        .context("cannot start SecretBroker MCP transport")?
        .waiting()
        .await
        .context("SecretBroker MCP transport failed")?;
    Ok(())
}

impl ServerHandler for SecretBrokerMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(
            Implementation::new("secretbroker", env!("CARGO_PKG_VERSION"))
                .with_title("SecretBroker")
                .with_description(
                    "Opens secure local credential input and reports metadata without reading values",
                )
                .with_website_url("https://github.com/HamedMP/secretbroker"),
        )
        .with_instructions(
            "Never request secret values through MCP arguments. Pass only environment variable names and non-sensitive descriptions.",
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(vec![
            open_tool(),
            status_tool(),
        ]))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let arguments = request.arguments.unwrap_or_default();
        let result = match request.name.as_ref() {
            "secretbroker_open" => open_secure_input(arguments, Arc::clone(&self.launches)).await,
            "secretbroker_status" => status(arguments, &self.launches).await,
            _ => {
                return Err(ErrorData::invalid_params("unknown SecretBroker tool", None));
            }
        };
        Ok(result.unwrap_or_else(tool_error))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new(WIDGET_URI, "secretbroker-request-status")
                .with_title("SecretBroker secure input status")
                .with_description("Safe request metadata and local browser launch controls")
                .with_mime_type("text/html;profile=mcp-app"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        if request.uri != WIDGET_URI {
            return Err(ErrorData::resource_not_found("resource not found", None));
        }
        let metadata = meta(json!({
            "ui": {
                "prefersBorder": true,
                "csp": {
                    "connectDomains": [],
                    "resourceDomains": []
                }
            },
            "openai/widgetDescription": "SecretBroker opens a separate one-time local browser form and shows credential readiness by variable name."
        }));
        let contents = ResourceContents::text(WIDGET_HTML, WIDGET_URI)
            .with_mime_type("text/html;profile=mcp-app")
            .with_meta(metadata);
        Ok(ReadResourceResult::new(vec![contents]))
    }
}

async fn open_secure_input(
    arguments: JsonObject,
    launches: Arc<Mutex<HashMap<String, LaunchRecord>>>,
) -> Result<CallToolResult> {
    let request: OpenRequest =
        serde_json::from_value(Value::Object(arguments)).context("invalid secure input request")?;
    let scope = validate_scope(&request.scope)?;
    if request.variables.is_empty() || request.variables.len() > MAX_VARIABLES {
        anyhow::bail!("request between 1 and {MAX_VARIABLES} variable names");
    }
    if !(1..=MAX_TIMEOUT_MINUTES).contains(&request.timeout_minutes) {
        anyhow::bail!("timeout_minutes must be between 1 and {MAX_TIMEOUT_MINUTES}");
    }
    if request
        .ttl_minutes
        .is_some_and(|minutes| !(1..=MAX_TTL_MINUTES).contains(&minutes))
    {
        anyhow::bail!("ttl_minutes must be between 1 and {MAX_TTL_MINUTES}");
    }

    let mut names = Vec::with_capacity(request.variables.len());
    let mut seen = HashSet::with_capacity(request.variables.len());
    let mut variable_args = Vec::with_capacity(request.variables.len());
    for variable in request.variables {
        let name = EnvName::from_str(&variable.name)?;
        if !seen.insert(name.clone()) {
            anyhow::bail!("duplicate variable names are not allowed");
        }
        let description = variable.description.unwrap_or_default();
        validate_description(&description)?;
        variable_args.push(if description.is_empty() {
            name.to_string()
        } else {
            format!("{name}={description}")
        });
        names.push(name.to_string());
    }

    let mut command = Command::new(std::env::current_exe().context("cannot locate executable")?);
    command
        .arg("--json")
        .arg("request")
        .arg("--scope")
        .arg(&request.scope)
        .arg("--web")
        .arg("--timeout")
        .arg(format!("{}m", request.timeout_minutes))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    if request.replace {
        command.arg("--replace");
    }
    if let Some(minutes) = request.ttl_minutes {
        command.arg("--ttl").arg(format!("{minutes}m"));
    }
    for variable in &variable_args {
        command.arg("--var").arg(variable);
    }
    let mut child = command
        .spawn()
        .context("cannot launch secure local input process")?;
    let key = launch_key(&scope, &names);
    let generation = ulid::Ulid::generate().to_string();
    launches.lock().await.insert(
        key.clone(),
        LaunchRecord {
            generation: generation.clone(),
            state: LaunchState::Running,
        },
    );
    tokio::spawn(async move {
        let succeeded = child.wait().await.is_ok_and(|status| status.success());
        let mut launches = launches.lock().await;
        let Some(record) = launches.get_mut(&key) else {
            return;
        };
        if record.generation != generation {
            return;
        }
        if succeeded {
            launches.remove(&key);
        } else {
            record.state = LaunchState::Failed;
        }
    });

    Ok(success_result(
        json!({
            "status": "starting",
            "scope": scope.label,
            "scopeArgument": request.scope,
            "variables": names
        }),
        "SecretBroker started a separate process to open the secure local browser form. Secret values must be entered only there.",
    ))
}

async fn status(
    arguments: JsonObject,
    launches: &Mutex<HashMap<String, LaunchRecord>>,
) -> Result<CallToolResult> {
    let request: StatusRequest =
        serde_json::from_value(Value::Object(arguments)).context("invalid status request")?;
    let scope = validate_scope(&request.scope)?;
    if request.variables.is_empty() || request.variables.len() > MAX_VARIABLES {
        anyhow::bail!("request between 1 and {MAX_VARIABLES} variable names");
    }
    let requested: Vec<EnvName> = request
        .variables
        .iter()
        .map(|name| EnvName::from_str(name))
        .collect::<Result<_, _>>()?;
    let paths = AppPaths::discover()?;
    let repository = SecretRepository::new(
        NativeCredentialStore,
        MetadataStore::new(paths.metadata_path(), paths.metadata_lock_path()),
    );
    let now = unix_timestamp();
    let metadata = repository.status(&scope)?;
    let mut available = Vec::new();
    let mut expired = Vec::new();
    let mut missing = Vec::new();
    for name in requested {
        match metadata.iter().find(|entry| entry.name == name) {
            Some(entry) if entry.is_expired_at(now) => expired.push(name.to_string()),
            Some(_) => available.push(name.to_string()),
            None => missing.push(name.to_string()),
        }
    }
    let state = if missing.is_empty() && expired.is_empty() {
        "ready"
    } else {
        "pending"
    };
    let launch_status = match launches
        .lock()
        .await
        .get(&launch_key(&scope, &request.variables))
        .map(|record| record.state)
    {
        Some(LaunchState::Running) => "running",
        Some(LaunchState::Failed) => "failed",
        None => "idle",
    };
    Ok(success_result(
        json!({
            "status": state,
            "launchStatus": launch_status,
            "scope": scope.label,
            "scopeArgument": request.scope,
            "variables": request.variables,
            "available": available,
            "missing": missing,
            "expired": expired
        }),
        "SecretBroker returned credential readiness metadata only.",
    ))
}

fn open_tool() -> Tool {
    Tool::new(
        "secretbroker_open",
        "Open SecretBroker's separate one-time local browser form for explicitly named environment variables. Never pass secret values or secret-bearing descriptions.",
        schema(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["variables"],
            "properties": {
                "scope": {
                    "type": "string",
                    "description": "SecretBroker scope: user, project, or session:<id>",
                    "default": "project"
                },
                "variables": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_VARIABLES,
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["name"],
                        "properties": {
                            "name": {"type": "string", "pattern": "^[A-Za-z_][A-Za-z0-9_]*$"},
                            "description": {"type": "string", "maxLength": MAX_DESCRIPTION_BYTES}
                        }
                    }
                },
                "replace": {"type": "boolean", "default": false},
                "timeout_minutes": {"type": "integer", "minimum": 1, "maximum": MAX_TIMEOUT_MINUTES, "default": 10},
                "ttl_minutes": {"type": "integer", "minimum": 1, "maximum": MAX_TTL_MINUTES}
            }
        })),
    )
    .with_title("Open secure credential input")
    .with_raw_output_schema(schema(json!({
        "type": "object",
        "required": ["status", "scope", "scopeArgument", "variables"],
        "properties": {
            "status": {"type": "string"},
            "scope": {"type": "string"},
            "scopeArgument": {"type": "string"},
            "variables": {"type": "array", "items": {"type": "string"}}
        }
    })))
    .with_annotations(
        ToolAnnotations::with_title("Open secure credential input")
            .read_only(false)
            .destructive(false)
            .idempotent(false)
            .open_world(false),
    )
    .with_meta(meta(json!({
        "ui": {"resourceUri": WIDGET_URI},
        "openai/outputTemplate": WIDGET_URI,
        "openai/toolInvocation/invoking": "Starting secure input…",
        "openai/toolInvocation/invoked": "Secure input started"
    })))
}

fn status_tool() -> Tool {
    Tool::new(
        "secretbroker_status",
        "Return readiness metadata for explicitly named credentials without retrieving secret values.",
        schema(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["variables"],
            "properties": {
                "scope": {"type": "string", "default": "project"},
                "variables": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_VARIABLES,
                    "items": {"type": "string", "pattern": "^[A-Za-z_][A-Za-z0-9_]*$"}
                }
            }
        })),
    )
    .with_title("Check credential readiness")
    .with_annotations(
        ToolAnnotations::with_title("Check credential readiness")
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
    .with_meta(meta(json!({
        "ui": {"visibility": ["app"]}
    })))
}

fn success_result(structured_content: Value, message: &str) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(message)]);
    result.structured_content = Some(structured_content);
    result
}

fn tool_error(error: anyhow::Error) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format!(
        "SecretBroker request failed: {error}"
    ))])
}

fn schema(value: Value) -> Arc<JsonObject> {
    Arc::new(value.as_object().expect("schema must be an object").clone())
}

fn meta(value: Value) -> Meta {
    Meta(
        value
            .as_object()
            .expect("metadata must be an object")
            .clone(),
    )
}

fn default_scope() -> String {
    "project".to_owned()
}

const fn default_timeout_minutes() -> u64 {
    10
}

fn validate_scope(value: &str) -> Result<Scope> {
    Scope::parse(value, &std::env::current_dir()?)
}

fn launch_key(scope: &Scope, names: &[String]) -> String {
    let mut names = names.to_vec();
    names.sort_unstable();
    format!("{}:{}", scope.id, names.join(","))
}

fn validate_description(value: &str) -> Result<()> {
    if value.len() > MAX_DESCRIPTION_BYTES {
        anyhow::bail!("variable descriptions may contain at most {MAX_DESCRIPTION_BYTES} bytes");
    }
    if value.chars().any(char::is_control) {
        anyhow::bail!("variable descriptions may not contain control characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_DESCRIPTION_BYTES, launch_key, meta, open_tool, status_tool, validate_description,
    };
    use crate::scope::Scope;
    use serde_json::json;

    #[test]
    fn app_status_tool_is_not_model_visible() {
        let tool = status_tool();
        assert_eq!(
            tool.meta
                .expect("metadata")
                .0
                .get("ui")
                .and_then(|value| value.get("visibility")),
            Some(&json!(["app"]))
        );
    }

    #[test]
    fn open_tool_links_only_to_safe_widget_resource() {
        let tool = open_tool();
        let metadata = tool.meta.expect("metadata");
        assert_eq!(
            metadata.0.get("openai/outputTemplate"),
            Some(&json!("ui://secretbroker/request-status.html"))
        );
        assert!(
            !serde_json::to_string(&metadata)
                .expect("serialize")
                .contains("http://")
        );
    }

    #[test]
    fn descriptions_are_bounded_and_reject_control_characters() {
        assert!(validate_description("Deployment token").is_ok());
        assert!(validate_description("line\nbreak").is_err());
        assert!(validate_description(&"x".repeat(MAX_DESCRIPTION_BYTES + 1)).is_err());
    }

    #[test]
    fn metadata_helper_does_not_modify_values() {
        assert_eq!(
            meta(json!({"safe": true})).0.get("safe"),
            Some(&json!(true))
        );
    }

    #[test]
    fn launch_tracking_key_is_order_independent() {
        let scope = Scope::session("mcp-test").expect("scope");
        assert_eq!(
            launch_key(&scope, &["B".to_owned(), "A".to_owned()]),
            launch_key(&scope, &["A".to_owned(), "B".to_owned()])
        );
    }
}
