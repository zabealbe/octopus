//! The octopus skill: every command and flag of the command tree, rendered for an agent to read, installed for the
//! user's Claude Code, and handed to the agents a server runs: through the harness files it lays out, and in
//! `~/.agents/skills` for Codex and OpenCode.

use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use clap::{Command, Subcommand};
use serde::Serialize;

use crate::print;

/// Where the rendered skill lives, relative to the workspace root.
pub const PATH: &str = "skills/octopus/SKILL.md";

const HEAD: &str = r#"---
name: octopus
description: Drive octopus servers from a shell, covering workspaces, tasks, agent terminals, files, browser panels and pairing. Use when an agent or script creates, moves or inspects octopus tasks, types into their terminals, or opens a page in a task's browser panel.
---

# octopus

Every flag is required. A success prints the op's result as JSON on stdout; a failure prints `error: ` and the reason on stderr and exits non-zero. Inside a task terminal, pass the terminal's own server and task: `--server "$OCTOPUS_SERVER" --task "$OCTOPUS_TASK"`.
"#;

fn value(arg: &clap::Arg) -> String {
    let choices: Vec<String> = arg.get_possible_values().iter().map(|choice| choice.get_name().to_string()).collect();
    if choices.is_empty() {
        let names = arg.get_value_names().unwrap_or_default();
        names.iter().map(|name| format!("<{}>", name.to_lowercase().replace('_', "-"))).collect::<Vec<_>>().join(" ")
    } else {
        choices.join("|")
    }
}

/// Clap drops the closing period of a derived doc comment.
fn sentence(text: impl ToString) -> String {
    let text = text.to_string();
    if text.ends_with('.') { text } else { format!("{text}.") }
}

fn render_leaf(path: &str, command: &Command, out: &mut String) {
    out.push_str(&format!("\n## `octopus {path}`\n\n"));
    if let Some(about) = command.get_about() {
        out.push_str(&format!("{}\n", sentence(about)));
    }
    let flags: Vec<&clap::Arg> = command.get_arguments().filter(|arg| arg.get_long().is_some()).collect();
    if !flags.is_empty() {
        out.push('\n');
    }
    for arg in flags {
        let long = arg.get_long().expect("filtered to long flags");
        let help = sentence(arg.get_help().expect("every flag is documented"));
        out.push_str(&format!("- `--{long} {}`: {help}\n", value(arg)));
    }
}

fn render_tree(path: &str, command: &Command, out: &mut String) {
    if command.is_hide_set() {
        return;
    }
    if command.has_subcommands() {
        for sub in command.get_subcommands() {
            let sub_path =
                if path.is_empty() { sub.get_name().to_string() } else { format!("{path} {}", sub.get_name()) };
            render_tree(&sub_path, sub, out);
        }
    } else {
        render_leaf(path, command, out);
    }
}

pub fn render(tree: &Command) -> String {
    let mut out = HEAD.to_string();
    render_tree("", tree, &mut out);
    out
}

/// Writes `text` to `path`, making its folders.
fn write(path: &Path, text: &str) -> anyhow::Result<()> {
    let folder = path.parent().expect("a skill file sits in a folder");
    std::fs::create_dir_all(folder).with_context(|| format!("create {}", folder.display()))?;
    std::fs::write(path, text).with_context(|| format!("write {}", path.display()))
}

fn home() -> anyhow::Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| anyhow!("no home directory"))
}

/// The folder Claude Code reads the user's octopus skill from.
fn user_skill() -> anyhow::Result<PathBuf> {
    Ok(home()?.join(".claude/skills/octopus"))
}

/// Writes the skill this binary renders to `~/.agents/skills/octopus/SKILL.md`, where Codex and OpenCode read it.
pub fn install_for_agents() -> anyhow::Result<()> {
    write(&home()?.join(".agents/skills/octopus/SKILL.md"), &render(&crate::commands::tree()))
}

#[derive(Serialize)]
struct Placed {
    path: PathBuf,
}

/// Writes the skill this binary renders to `~/.claude/skills/octopus/SKILL.md`, returning the file's path.
pub fn install() -> anyhow::Result<PathBuf> {
    let path = user_skill()?.join("SKILL.md");
    write(&path, &render(&crate::commands::tree()))?;
    Ok(path)
}

/// Removes the skill's folder from `~/.claude/skills`, returning its path.
fn uninstall() -> anyhow::Result<PathBuf> {
    let folder = user_skill()?;
    match std::fs::remove_dir_all(&folder) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(error).with_context(|| format!("remove {}", folder.display()))
        }
        _ => Ok(folder),
    }
}

#[derive(Subcommand)]
pub enum SkillVerb {
    /// Write the octopus skill, rendered from this octopus's commands, to ~/.claude/skills/octopus/SKILL.md for Claude Code.
    Install,
    /// Remove the octopus skill from ~/.claude/skills.
    Uninstall,
}

impl SkillVerb {
    pub fn run(self) -> anyhow::Result<()> {
        let path = match self {
            Self::Install => install()?,
            Self::Uninstall => uninstall()?,
        };
        print(&Placed { path })
    }
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    #[test]
    fn the_skill_documents_the_current_commands() {
        let committed = include_str!("../../../skills/octopus/SKILL.md");
        assert!(
            committed == super::render(&crate::commands::tree()),
            "{} drifted from the commands; run `cargo run -p octopus-cli --example skill`",
            super::PATH
        );
    }
}
