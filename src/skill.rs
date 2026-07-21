use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

const SKILL_MD: &str = include_str!("../skills/secretbroker/SKILL.md");
const WORKFLOW_MD: &str = include_str!("../skills/secretbroker/references/workflow.md");
const SECURITY_MD: &str = include_str!("../skills/secretbroker/references/security.md");

#[derive(Clone, Copy, Debug)]
pub enum Agent {
    Pi,
    Claude,
    Codex,
    All,
}

#[derive(Debug, Serialize)]
pub struct InstallResult {
    pub installed: Vec<String>,
}

pub fn install(agent: Agent, global: bool, force: bool, cwd: &Path) -> Result<InstallResult> {
    let targets = targets(agent, global, cwd)?;
    let mut installed = Vec::new();
    for target in targets {
        install_target(&target, force)?;
        installed.push(target.display().to_string());
    }
    Ok(InstallResult { installed })
}

fn targets(agent: Agent, global: bool, cwd: &Path) -> Result<Vec<PathBuf>> {
    let home = directories::UserDirs::new().context("cannot determine user home directory")?;
    let portable = if global {
        home.home_dir().join(".agents/skills/secretbroker")
    } else {
        cwd.join(".agents/skills/secretbroker")
    };
    let claude = if global {
        home.home_dir().join(".claude/skills/secretbroker")
    } else {
        cwd.join(".claude/skills/secretbroker")
    };

    Ok(match agent {
        Agent::Pi | Agent::Codex => vec![portable],
        Agent::Claude => vec![claude],
        Agent::All => vec![portable, claude],
    })
}

fn install_target(target: &Path, force: bool) -> Result<()> {
    if target.exists() && !force {
        let current = fs::read_to_string(target.join("SKILL.md")).unwrap_or_default();
        if current != SKILL_MD {
            bail!(
                "{} already exists and differs; use --force to replace it",
                target.display()
            );
        }
    }
    fs::create_dir_all(target.join("references"))
        .with_context(|| format!("cannot create {}", target.display()))?;
    fs::write(target.join("SKILL.md"), SKILL_MD)?;
    fs::write(target.join("references/workflow.md"), WORKFLOW_MD)?;
    fs::write(target.join("references/security.md"), SECURITY_MD)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Agent, install};

    #[test]
    fn installs_project_skill_for_portable_agents() {
        let temporary = tempfile::tempdir().expect("temp directory");
        let result = install(Agent::Codex, false, false, temporary.path()).expect("install");
        assert_eq!(result.installed.len(), 1);
        assert!(
            temporary
                .path()
                .join(".agents/skills/secretbroker/SKILL.md")
                .exists()
        );
    }

    #[test]
    fn installs_all_agents_without_duplicate_portable_targets() {
        let temporary = tempfile::tempdir().expect("temp directory");
        let result = install(Agent::All, false, false, temporary.path()).expect("install");
        assert_eq!(result.installed.len(), 2);
        assert!(
            temporary
                .path()
                .join(".agents/skills/secretbroker/SKILL.md")
                .exists()
        );
        assert!(
            temporary
                .path()
                .join(".claude/skills/secretbroker/SKILL.md")
                .exists()
        );
    }

    #[test]
    fn refuses_to_overwrite_modified_skill_without_force() {
        let temporary = tempfile::tempdir().expect("temp directory");
        install(Agent::Pi, false, false, temporary.path()).expect("initial install");
        std::fs::write(
            temporary
                .path()
                .join(".agents/skills/secretbroker/SKILL.md"),
            "modified",
        )
        .expect("modify skill");
        assert!(install(Agent::Pi, false, false, temporary.path()).is_err());
        install(Agent::Pi, false, true, temporary.path()).expect("forced install");
    }
}
