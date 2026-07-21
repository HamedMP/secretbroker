use std::{
    collections::HashSet,
    ffi::OsString,
    io::{self, IsTerminal},
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

use anyhow::{Result, bail};
use clap::{ArgAction, ArgGroup, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use dialoguer::{Confirm, Select, theme::ColorfulTheme};
use serde::Serialize;

use crate::{
    collector::{VariableRequest, terminal, web},
    env_name::EnvName,
    metadata::{MetadataStore, unix_timestamp},
    output,
    paths::AppPaths,
    pending::{PendingStore, RequestState},
    runner,
    scope::Scope,
    skill::{self, Agent},
    storage::{NativeCredentialStore, SecretRepository},
};

#[derive(Debug, Parser)]
#[command(
    name = "secretbroker",
    version,
    about = "Broker secrets to local commands without putting values in agent context"
)]
pub struct Cli {
    #[arg(long, global = true, help = "Emit versioned JSON/NDJSON metadata only")]
    pub json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Request one or more secrets through a secure local input flow.
    Request(RequestArgs),
    /// Fulfill a pending request from this terminal.
    Fulfill(FulfillArgs),
    /// Wait until a pending request is fulfilled.
    Wait(WaitArgs),
    /// Run a command with explicitly named secrets in its environment.
    Run(RunArgs),
    /// List credential names and expiry metadata without retrieving values.
    Status(ScopeArgs),
    /// Delete explicitly named credentials.
    Unset(UnsetArgs),
    /// Delete every credential in a scope.
    Clear(ClearArgs),
    /// Install the portable Agent Skill.
    Init(InitArgs),
    /// Generate shell completion definitions.
    Completions { shell: Shell },
    /// Check local configuration and platform support.
    Doctor,
}

#[derive(Clone, Debug, Args)]
struct ScopeArgs {
    #[arg(
        long,
        default_value = "project",
        help = "user, project, or session:<id>"
    )]
    scope: String,
    #[arg(
        long,
        value_name = "PATH",
        help = "Project directory used to derive project scope"
    )]
    project_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("mode").args(["terminal", "web", "another_terminal"]).multiple(false)))]
struct RequestArgs {
    #[command(flatten)]
    scope: ScopeArgs,
    #[arg(long = "var", required = true, value_parser = parse_variable_request)]
    variables: Vec<VariableRequest>,
    #[arg(long)]
    terminal: bool,
    #[arg(long)]
    web: bool,
    #[arg(long = "another-terminal")]
    another_terminal: bool,
    #[arg(long, help = "Wait for another-terminal fulfillment")]
    wait: bool,
    #[arg(long, default_value = "10m", value_parser = parse_duration)]
    timeout: Duration,
    #[arg(long, value_parser = parse_duration, help = "Credential lifetime, for example 30m or 8h")]
    ttl: Option<Duration>,
    #[arg(long)]
    replace: bool,
    #[arg(long)]
    allow_empty: bool,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("mode").args(["terminal", "web"]).multiple(false)))]
struct FulfillArgs {
    request_id: String,
    #[arg(long)]
    terminal: bool,
    #[arg(long)]
    web: bool,
    #[arg(long)]
    allow_empty: bool,
    #[arg(long, value_parser = parse_duration)]
    ttl: Option<Duration>,
}

#[derive(Debug, Args)]
struct WaitArgs {
    request_id: String,
    #[arg(long, default_value = "10m", value_parser = parse_duration)]
    timeout: Duration,
}

#[derive(Debug, Args)]
struct RunArgs {
    #[command(flatten)]
    scope: ScopeArgs,
    #[arg(long = "with", required = true)]
    variables: Vec<EnvName>,
    #[arg(last = true, required = true, allow_hyphen_values = true)]
    command: Vec<OsString>,
}

#[derive(Debug, Args)]
struct UnsetArgs {
    #[command(flatten)]
    scope: ScopeArgs,
    #[arg(required = true)]
    variables: Vec<EnvName>,
}

#[derive(Debug, Args)]
struct ClearArgs {
    #[command(flatten)]
    scope: ScopeArgs,
    #[arg(long, action = ArgAction::SetTrue)]
    yes: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AgentArg {
    Pi,
    Claude,
    Codex,
    All,
}

#[derive(Debug, Args)]
struct InitArgs {
    #[arg(long, value_enum)]
    agent: AgentArg,
    #[arg(long, conflicts_with = "project")]
    global: bool,
    #[arg(long, conflicts_with = "global")]
    project: bool,
    #[arg(long)]
    force: bool,
}

#[derive(Clone, Copy, Debug)]
enum CollectionMode {
    Browser,
    Terminal,
    AnotherTerminal,
}

#[derive(Debug, Serialize)]
struct ReadyOutput {
    version: u32,
    status: &'static str,
    scope: String,
    available: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RequestOutput {
    version: u32,
    status: &'static str,
    request_id: String,
    fulfill_command: String,
    expires_at: u64,
    variables: Vec<String>,
}

#[derive(Debug, Serialize)]
struct StatusOutput {
    version: u32,
    scope: String,
    credentials: Vec<StatusEntry>,
}

#[derive(Debug, Serialize)]
struct StatusEntry {
    name: String,
    created_at: u64,
    expires_at: Option<u64>,
    expired: bool,
}

#[derive(Debug, Serialize)]
struct CountOutput {
    version: u32,
    status: &'static str,
    count: usize,
}

#[derive(Debug, Serialize)]
struct DoctorOutput {
    version: u32,
    status: &'static str,
    platform: &'static str,
    data_dir: String,
    runtime_dir: String,
}

pub async fn execute(cli: Cli) -> Result<i32> {
    let json = cli.json;
    match cli.command {
        Command::Request(args) => request(args, json).await?,
        Command::Fulfill(args) => fulfill(args, json).await?,
        Command::Wait(args) => wait(args, json).await?,
        Command::Run(args) => return run(args).await,
        Command::Status(args) => status(args, json)?,
        Command::Unset(args) => unset(args, json)?,
        Command::Clear(args) => clear(args, json)?,
        Command::Init(args) => init(args, json)?,
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "secretbroker",
                &mut io::stdout(),
            );
        }
        Command::Doctor => doctor(json)?,
    }
    Ok(0)
}

async fn request(args: RequestArgs, json: bool) -> Result<()> {
    validate_unique_variables(&args.variables)?;
    let paths = AppPaths::discover()?;
    let scope = resolve_scope(&args.scope)?;
    let repository = repository(&paths);
    let now = unix_timestamp();
    let existing = repository.status(&scope)?;
    let variables: Vec<_> = args
        .variables
        .iter()
        .filter(|variable| {
            args.replace
                || !existing
                    .iter()
                    .any(|entry| entry.name == variable.name && !entry.is_expired_at(now))
        })
        .cloned()
        .collect();

    if variables.is_empty() {
        let names = existing
            .into_iter()
            .filter(|entry| !entry.is_expired_at(now))
            .map(|entry| entry.name.to_string())
            .collect();
        return print_ready(json, &scope, names);
    }

    let mode = choose_request_mode(&args)?;
    if matches!(mode, CollectionMode::AnotherTerminal) {
        let pending = PendingStore::new(&paths);
        let request = pending.create(scope.clone(), variables.clone(), args.timeout)?;
        let event = RequestOutput {
            version: 1,
            status: "pending",
            request_id: request.id.clone(),
            fulfill_command: format!("secretbroker fulfill {}", request.id),
            expires_at: request.expires_at,
            variables: variables.iter().map(|item| item.name.to_string()).collect(),
        };
        output::print(json, &event, |event| {
            format!(
                "Request {} is waiting. In another terminal run:\n\n  {}\n",
                event.request_id, event.fulfill_command
            )
        })?;
        if args.wait {
            let completed = pending.wait(&request.id, args.timeout).await?;
            return print_ready(
                json,
                &completed.scope,
                completed
                    .variables
                    .iter()
                    .map(|item| item.name.to_string())
                    .collect(),
            );
        }
        return Ok(());
    }

    let values = collect(mode, &variables, args.allow_empty, args.timeout).await?;
    let expiry = args
        .ttl
        .map(|ttl| unix_timestamp().saturating_add(ttl.as_secs()));
    let stored = repository.store_many(&scope, values, expiry)?;
    print_ready(
        json,
        &scope,
        stored
            .into_iter()
            .map(|entry| entry.name.to_string())
            .collect(),
    )
}

async fn fulfill(args: FulfillArgs, json: bool) -> Result<()> {
    let paths = AppPaths::discover()?;
    let pending = PendingStore::new(&paths);
    let request = pending.load(&args.request_id)?;
    if request.state != RequestState::Pending {
        bail!("request {} is not pending", request.id);
    }
    let mode = if args.web {
        CollectionMode::Browser
    } else if args.terminal {
        CollectionMode::Terminal
    } else {
        choose_local_mode()?
    };
    let lifetime = Duration::from_secs(request.expires_at.saturating_sub(unix_timestamp()));
    let values = collect(mode, &request.variables, args.allow_empty, lifetime).await?;
    let expiry = args
        .ttl
        .map(|ttl| unix_timestamp().saturating_add(ttl.as_secs()));
    repository(&paths).store_many(&request.scope, values, expiry)?;
    let completed = pending.transition(&request.id, RequestState::Pending, RequestState::Ready)?;
    print_ready(
        json,
        &completed.scope,
        completed
            .variables
            .iter()
            .map(|item| item.name.to_string())
            .collect(),
    )
}

async fn wait(args: WaitArgs, json: bool) -> Result<()> {
    let paths = AppPaths::discover()?;
    let request = PendingStore::new(&paths)
        .wait(&args.request_id, args.timeout)
        .await?;
    print_ready(
        json,
        &request.scope,
        request
            .variables
            .iter()
            .map(|item| item.name.to_string())
            .collect(),
    )
}

async fn run(args: RunArgs) -> Result<i32> {
    let paths = AppPaths::discover()?;
    let scope = resolve_scope(&args.scope)?;
    let status = runner::run(&repository(&paths), &scope, &args.variables, &args.command).await?;
    Ok(status.code().unwrap_or(1))
}

fn status(args: ScopeArgs, json: bool) -> Result<()> {
    let paths = AppPaths::discover()?;
    let scope = resolve_scope(&args)?;
    let now = unix_timestamp();
    let credentials = repository(&paths)
        .status(&scope)?
        .into_iter()
        .map(|entry| StatusEntry {
            name: entry.name.to_string(),
            created_at: entry.created_at,
            expires_at: entry.expires_at,
            expired: entry.is_expired_at(now),
        })
        .collect();
    let result = StatusOutput {
        version: 1,
        scope: scope.label,
        credentials,
    };
    output::print(json, &result, |result| {
        if result.credentials.is_empty() {
            return format!("No credentials in {}.", result.scope);
        }
        result
            .credentials
            .iter()
            .map(|entry| {
                let state = if entry.expired { "expired" } else { "ready" };
                format!("{}\t{state}", entry.name)
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn unset(args: UnsetArgs, json: bool) -> Result<()> {
    let paths = AppPaths::discover()?;
    let scope = resolve_scope(&args.scope)?;
    let count = args.variables.len();
    repository(&paths).delete(&scope, &args.variables)?;
    let result = CountOutput {
        version: 1,
        status: "deleted",
        count,
    };
    output::print(json, &result, |result| {
        format!("Deleted {} credential(s).", result.count)
    })
}

fn clear(args: ClearArgs, json: bool) -> Result<()> {
    let paths = AppPaths::discover()?;
    let scope = resolve_scope(&args.scope)?;
    if !args.yes {
        if json || !io::stdin().is_terminal() {
            bail!("clear requires --yes in non-interactive mode");
        }
        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Delete every credential in {scope}?"))
            .default(false)
            .interact()?;
        if !confirmed {
            bail!("cancelled");
        }
    }
    let count = repository(&paths).clear(&scope)?;
    let result = CountOutput {
        version: 1,
        status: "deleted",
        count,
    };
    output::print(json, &result, |result| {
        format!("Deleted {} credential(s).", result.count)
    })
}

fn init(args: InitArgs, json: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let global = args.global && !args.project;
    let agent = match args.agent {
        AgentArg::Pi => Agent::Pi,
        AgentArg::Claude => Agent::Claude,
        AgentArg::Codex => Agent::Codex,
        AgentArg::All => Agent::All,
    };
    let result = skill::install(agent, global, args.force, &cwd)?;
    output::print(json, &result, |result| {
        format!(
            "Installed SecretBroker skill:\n{}",
            result.installed.join("\n")
        )
    })
}

fn doctor(json: bool) -> Result<()> {
    let paths = AppPaths::discover()?;
    let result = DoctorOutput {
        version: 1,
        status: "ok",
        platform: std::env::consts::OS,
        data_dir: paths.data_dir.display().to_string(),
        runtime_dir: paths.runtime_dir.display().to_string(),
    };
    output::print(json, &result, |result| {
        format!(
            "SecretBroker is ready on {}.\nData: {}\nRuntime: {}",
            result.platform, result.data_dir, result.runtime_dir
        )
    })
}

async fn collect(
    mode: CollectionMode,
    variables: &[VariableRequest],
    allow_empty: bool,
    lifetime: Duration,
) -> Result<Vec<crate::collector::CollectedSecret>> {
    match mode {
        CollectionMode::Browser => web::collect(variables, allow_empty, lifetime).await,
        CollectionMode::Terminal => terminal::collect(variables, allow_empty),
        CollectionMode::AnotherTerminal => {
            bail!("another-terminal mode must use a pending request")
        }
    }
}

fn choose_request_mode(args: &RequestArgs) -> Result<CollectionMode> {
    if args.web {
        Ok(CollectionMode::Browser)
    } else if args.terminal {
        Ok(CollectionMode::Terminal)
    } else if args.another_terminal {
        Ok(CollectionMode::AnotherTerminal)
    } else {
        choose_mode(true)
    }
}

fn choose_local_mode() -> Result<CollectionMode> {
    choose_mode(false)
}

fn choose_mode(include_another_terminal: bool) -> Result<CollectionMode> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        bail!("choose --web, --terminal, or --another-terminal in non-interactive mode");
    }
    let mut labels = vec![
        "Open a secure browser form",
        "Enter values in this terminal",
    ];
    if include_another_terminal {
        labels.push("Enter values from another terminal");
    }
    labels.push("Cancel");
    let selected = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("How would you like to provide the requested secrets?")
        .items(&labels)
        .default(0)
        .interact()?;
    match selected {
        0 => Ok(CollectionMode::Browser),
        1 => Ok(CollectionMode::Terminal),
        2 if include_another_terminal => Ok(CollectionMode::AnotherTerminal),
        _ => bail!("cancelled"),
    }
}

fn resolve_scope(args: &ScopeArgs) -> Result<Scope> {
    let project_dir = args
        .project_dir
        .clone()
        .map_or_else(std::env::current_dir, Ok)?;
    Scope::parse(&args.scope, &project_dir)
}

fn repository(paths: &AppPaths) -> SecretRepository<NativeCredentialStore> {
    SecretRepository::new(
        NativeCredentialStore,
        MetadataStore::new(paths.metadata_path(), paths.metadata_lock_path()),
    )
}

fn parse_variable_request(value: &str) -> Result<VariableRequest, String> {
    let (name, description) = value
        .split_once('=')
        .map_or((value, None), |(name, description)| {
            (
                name,
                (!description.is_empty()).then(|| description.to_owned()),
            )
        });
    let name = EnvName::from_str(name).map_err(|error| error.to_string())?;
    Ok(VariableRequest { name, description })
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    humantime::parse_duration(value).map_err(|error| error.to_string())
}

fn validate_unique_variables(variables: &[VariableRequest]) -> Result<()> {
    let mut names = HashSet::with_capacity(variables.len());
    for variable in variables {
        if !names.insert(variable.name.clone()) {
            bail!("{} was requested more than once", variable.name);
        }
    }
    Ok(())
}

fn print_ready(json: bool, scope: &Scope, available: Vec<String>) -> Result<()> {
    let result = ReadyOutput {
        version: 1,
        status: "ready",
        scope: scope.label.clone(),
        available,
    };
    output::print(json, &result, |result| {
        format!("Ready in {}: {}", result.scope, result.available.join(", "))
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_variable_request, validate_unique_variables};

    #[test]
    fn parses_name_and_description() {
        let request = parse_variable_request("TOKEN=Deployment token").expect("request");
        assert_eq!(request.name.as_str(), "TOKEN");
        assert_eq!(request.description.as_deref(), Some("Deployment token"));
    }

    #[test]
    fn rejects_duplicate_variables() {
        let request = parse_variable_request("TOKEN").expect("request");
        assert!(validate_unique_variables(&[request.clone(), request]).is_err());
    }
}
