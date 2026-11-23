//! Each task's agent: which harness runs it, when it starts, the status it reports and when it is reclaimed.

use std::path::PathBuf;
use std::sync::{Arc, Weak};

use octopus_proto::Error;
use octopus_proto::Event;
use octopus_proto::ops::{AgentReportSession, HarnessCheck, HarnessSet, HarnessStatus, Harnesses, Revised, TaskFocus};
use octopus_proto::types::{AGENT_PANEL, AgentStatus, PtyId, TaskId};

use crate::program_status::ProgramStatus;
use crate::store::{Harness, PanelRow, TaskRow};
use crate::terminals::Terminal;
use crate::{Server, State, harness, lock, login, nonempty, now, report};

/// How long an agent stays idle, silent and unshown before the server stops it to reclaim its memory.
pub const AGENT_RECLAIM_MS: i64 = 30 * 60 * 1000;

pub(crate) enum AgentSlot {
    /// The agent's terminal, running or ended, with the status the agent last reported and when it took it.
    Spawned {
        terminal: Arc<Terminal>,
        status: AgentStatus,
        since: i64,
    },
    HarnessNotInstalled {
        install: String,
    },
    HarnessUnknown,
}

impl AgentSlot {
    pub(crate) fn terminal(&self) -> Option<&Arc<Terminal>> {
        match self {
            Self::Spawned { terminal, .. } => Some(terminal),
            _ => None,
        }
    }
}

impl State {
    /// Ends the task's agent terminal and drops its scrollback.
    pub(crate) fn forget_agent(&mut self, task: &TaskId) {
        if let Some(AgentSlot::Spawned { terminal, .. }) = self.agents.remove(task)
            && let Err(error) = terminal.forget()
        {
            report!("forget the agent terminal of {task}: {error}");
        }
    }

    fn agent_running(&self, task: &TaskId) -> bool {
        self.agent_terminal(task).is_some_and(|terminal| terminal.running())
    }

    /// The agent's terminal when the agent runs and has shown idle, with every terminal of its task unshown, for
    /// `AGENT_RECLAIM_MS` at `now`.
    fn idle_unshown_agent(&self, task: &TaskId, now: i64) -> Option<Arc<Terminal>> {
        let Some(AgentSlot::Spawned { terminal, status: AgentStatus::Idle, since }) = self.agents.get(task) else {
            return None;
        };
        let unshown = self.terminals(task).map(|(_, terminal)| terminal.unshown_since()).collect::<Option<Vec<_>>>()?;
        let quiet_since = unshown.into_iter().fold(*since, i64::max);
        (terminal.running() && now - quiet_since >= AGENT_RECLAIM_MS).then(|| Arc::clone(terminal))
    }

    pub(crate) fn agent_terminal(&self, task: &TaskId) -> Option<&Arc<Terminal>> {
        self.agents.get(task).and_then(AgentSlot::terminal)
    }

    /// The harness `harness.set` stored under `id`, else the built-in one.
    fn harness(&self, id: &str) -> Result<Option<Harness>, Error> {
        Ok(self.store.harness(id)?.or_else(|| harness::built_in(id)))
    }
}

impl Server {
    pub(crate) fn harness_set(&self, op: HarnessSet) -> Result<Revised, Error> {
        if op.argv.is_empty() {
            return Err(Error::invalid("argv is empty"));
        }
        let harness = Harness { argv: op.argv, resume: op.resume, env: op.env, install: op.install };
        let mut state = lock(&self.state);
        let rev = state.store.set_harness(&op.id, &harness)?;
        self.publish(&state, rev, Event::HarnessUpdated { harness: op.id });
        Ok(Revised { rev })
    }

    pub(crate) fn harness_list(&self) -> Result<Harnesses, Error> {
        let mut harnesses = lock(&self.state).store.harness_ids()?;
        harnesses.extend(harness::built_ins().into_iter().map(|(id, _)| id.to_string()));
        harnesses.sort();
        harnesses.dedup();
        Ok(Harnesses { harnesses })
    }

    /// Looks the program up on the login PATH from the home folder, as an agent's start does from its directory.
    pub(crate) fn harness_check(&self, op: HarnessCheck) -> Result<HarnessStatus, Error> {
        let harness = lock(&self.state).harness(&op.id)?;
        let Harness { argv, install, .. } = harness.ok_or_else(|| Error::not_found(format!("harness {}", op.id)))?;
        let program = argv.into_iter().next().expect("harness.set rejects an empty argv");
        let home = login::home()?;
        let path = login::search_path(&login::resolve(&home)?)?;
        let installed = which::which_in(&program, Some(path), &home).is_ok();
        Ok(HarnessStatus { program, installed, install })
    }

    /// The agent in `terminal`, which shows idle until its program reports otherwise.
    pub(crate) fn spawned_agent(&self, task: &TaskId, terminal: Arc<Terminal>) -> AgentSlot {
        if terminal.running() {
            self.follow_agent(task.clone(), Arc::clone(&terminal));
        }
        AgentSlot::Spawned { terminal, status: AgentStatus::Idle, since: now() }
    }

    /// Follows what the agent in `terminal` reports it is doing until it exits.
    fn follow_agent(&self, task: TaskId, terminal: Arc<Terminal>) {
        let server = Weak::clone(&self.me);
        std::thread::spawn(move || {
            let statuses = match terminal.statuses() {
                Ok(statuses) => statuses,
                Err(Error::NotFound { .. }) => return,
                Err(error) => return report!("follow the status of the agent of {task}: {error}"),
            };
            for status in statuses {
                let Some(server) = server.upgrade() else { return };
                server.agent_reported(&task, &terminal.id, status);
            }
        });
    }

    fn agent_reported(&self, task: &TaskId, id: &PtyId, reported: ProgramStatus) {
        let shown = agent_status(reported);
        let mut state = lock(&self.state);
        let Some(AgentSlot::Spawned { terminal, status, since }) = state.agents.get_mut(task) else { return };
        if terminal.id != *id || *status == shown {
            return;
        }
        (*status, *since) = (shown, now());
        let saved = state.store.task(task).and_then(|mut row| self.save_and_publish(&mut state, &mut row));
        if let Err(error) = saved {
            report!("publish what the agent of {task} is doing: {error}");
        }
    }

    /// Records the session the task's agent reports it runs in, leaving the agent running.
    pub(crate) fn agent_report_session(&self, op: AgentReportSession) -> Result<Revised, Error> {
        nonempty("a session id", &op.session.0)?;
        let mut state = lock(&self.state);
        let mut row = state.store.task(&op.task)?;
        if row.session.as_ref() == Some(&op.session) {
            return Ok(Revised { rev: row.rev });
        }
        row.session = Some(op.session);
        self.save_and_publish(&mut state, &mut row)
    }

    /// Starts the agent of a live task whose agent is not running, in the task's directory once it is there. The login
    /// shell runs outside the state lock, so a slow profile stalls no other op; the task is read again after it, in
    /// case another focus or a move landed meanwhile.
    pub(crate) fn task_focus(&self, op: TaskFocus) -> Result<Revised, Error> {
        let (directory, harness) = {
            let (mut state, row) = self.ready_task(&op.task, |row| row.state.is_live())?;
            if !row.state.is_live() {
                return Ok(Revised { rev: row.rev });
            }
            self.watch_directory(&mut state, &row)?;
            if state.agent_running(&row.id) {
                return Ok(Revised { rev: row.rev });
            }
            (PathBuf::from(&row.directory), state.harness(&row.harness)?)
        };
        let started = harness.map(|harness| login::resolve(&directory).map(|env| (harness, env))).transpose()?;
        let mut state = lock(&self.state);
        let mut row = state.store.task(&op.task)?;
        if !row.state.is_live() || state.agent_running(&row.id) {
            return Ok(Revised { rev: row.rev });
        }
        state.forget_agent(&row.id);
        let slot = match started {
            Some((harness, env)) => self.start_agent(&mut state, &mut row, harness, op.args, env)?,
            None => AgentSlot::HarnessUnknown,
        };
        let rev = state.store.save_task(&mut row)?;
        state.agents.insert(row.id.clone(), slot);
        self.publish_task(&state, &row);
        Ok(Revised { rev })
    }

    /// Looks the program up on the agent's own PATH, so a harness counts as installed exactly when its agent can run.
    fn start_agent(
        &self,
        state: &mut State,
        row: &mut TaskRow,
        harness: Harness,
        focus_args: Vec<String>,
        mut env: Vec<(String, String)>,
    ) -> Result<AgentSlot, Error> {
        let directory = PathBuf::from(&row.directory);
        let (program, args) = harness.argv.split_first().expect("harness.set rejects an empty argv");
        let Ok(program) = which::which_in(program, Some(login::search_path(&env)?), &directory) else {
            return Ok(AgentSlot::HarnessNotInstalled { install: harness.install });
        };
        let files = self.files.to_string_lossy();
        let fill = |text: &String| text.replace(harness::FILES, &files);
        let resume = row.session.as_ref().map_or_else(Vec::new, |session| {
            harness.resume.iter().map(|arg| fill(arg).replace(harness::SESSION, &session.0)).collect()
        });
        let argv = std::iter::once(program.to_string_lossy().into_owned())
            .chain(args.iter().map(fill))
            .chain(focus_args)
            .chain(resume)
            .collect();
        env.extend(harness.env.iter().map(|var| (var.name.clone(), fill(&var.value))));
        let panel = PanelRow { id: PtyId::random(), task: row.id.clone(), name: AGENT_PANEL.to_string() };
        let terminal = self.spawn_panel(state, &panel, argv, &directory, env)?;
        Ok(self.spawned_agent(&row.id, terminal))
    }

    /// Stops every agent that has been idle, silent and unshown for `AGENT_RECLAIM_MS` at `now` and runs no detached
    /// descendant, returning their tasks. A stopped agent leaves its task's panels with its scrollback, which puts it
    /// asleep until the next focus resumes its session. Activity is read outside the state lock, and the stop is sent
    /// under it after a re-check; an agent whose activity the host cannot tell keeps running.
    pub fn reclaim_agents(&self, now: i64) -> Vec<TaskId> {
        let candidates: Vec<(TaskId, Arc<Terminal>)> = {
            let state = lock(&self.state);
            state.agents.keys().filter_map(|task| Some((task.clone(), state.idle_unshown_agent(task, now)?))).collect()
        };
        let mut reclaimed = Vec::new();
        for (task, terminal) in candidates {
            let activity = match terminal.activity() {
                Ok(activity) => activity,
                Err(error) if terminal.running() => {
                    report!("read the activity of the agent of {task}: {error}");
                    continue;
                }
                Err(_) => continue,
            };
            if activity.detached_descendant || now - activity.output_at < AGENT_RECLAIM_MS {
                continue;
            }
            let mut state = lock(&self.state);
            if state.idle_unshown_agent(&task, now).is_some_and(|current| current.id == terminal.id) {
                if let Err(error) = terminal.forget() {
                    report!("stop the idle agent of {task}: {error}");
                    continue;
                }
                state.agents.remove(&task);
                match self.touch_task(&mut state, &task) {
                    Ok(_) => reclaimed.push(task),
                    Err(error) => report!("publish the reclaimed agent of {task}: {error}"),
                }
            }
        }
        reclaimed
    }
}

/// An agent works while its program works, waits while its program is blocked on the user, and is idle otherwise.
fn agent_status(status: ProgramStatus) -> AgentStatus {
    match status {
        ProgramStatus::Working => AgentStatus::Working,
        ProgramStatus::Blocked => AgentStatus::Waiting,
        ProgramStatus::Done | ProgramStatus::Idle | ProgramStatus::Error | ProgramStatus::Clear => AgentStatus::Idle,
    }
}
