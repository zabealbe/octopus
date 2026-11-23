//! `octopus agent hook`: the command an agent's hook runs to report the session it runs in.

use anyhow::Context;
use clap::Subcommand;
use octopus_proto::ops::AgentReportSession;
use octopus_proto::types::{SessionId, TaskId};
use serde::Deserialize;

use crate::commands::SERVER_HELP;
use crate::{call, connect};

#[derive(Subcommand)]
pub enum AgentVerb {
    /// Report the session_id of the agent hook's JSON on stdin as the task's session. Prints nothing and exits 0
    /// whatever happens, printing a failure on stderr, so a report never blocks the agent.
    Hook {
        #[arg(long, help = SERVER_HELP)]
        server: String,
        /// The task's id.
        #[arg(long)]
        task: String,
    },
}

/// What a Claude Code or Codex hook, or the OpenCode plugin, hands its command; the hooks send more fields beside it.
#[derive(Deserialize)]
struct HookInput {
    session_id: String,
}

impl AgentVerb {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self::Hook { server, task } = self;
        if let Err(error) = report(&server, TaskId(task)).await {
            eprintln!("octopus agent hook: {error:#}");
        }
        Ok(())
    }
}

async fn report(server: &str, task: TaskId) -> anyhow::Result<()> {
    let input: HookInput = serde_json::from_reader(std::io::stdin()).context("read the hook's JSON on stdin")?;
    let connection = connect(server).await?;
    call(&connection, &AgentReportSession { task, session: SessionId(input.session_id) }).await?;
    Ok(())
}
