//! The task tree and its states: creating, moving, nesting, renaming and archiving tasks, and the view each publishes.

use std::path::{Path, PathBuf};

use octopus_proto::Error;
use octopus_proto::ops::{
    Revised, TaskArchive, TaskCreate, TaskCreated, TaskMove, TaskRename, TaskSetParent, TaskSetSession,
    TaskSetWorkspace,
};
use octopus_proto::types::{Agent, FRESH_SESSION, Mode, SessionStart, Task, TaskId, TaskState};

use crate::agents::AgentSlot;
use crate::store::TaskRow;
use crate::{Server, State, folders, lock, nonempty, now, report, worktrees};

pub fn ended_at(state: TaskState, now: i64) -> Option<i64> {
    (!state.is_live()).then_some(now)
}

impl State {
    pub(crate) fn view(&self, row: &TaskRow) -> Task {
        let agent = match self.agents.get(&row.id) {
            None => Agent::NotStarted,
            Some(AgentSlot::Spawned { terminal, status, .. }) if terminal.running() => {
                Agent::Running { status: *status }
            }
            Some(AgentSlot::Spawned { .. }) => Agent::NotStarted,
            Some(AgentSlot::HarnessNotInstalled { install }) => Agent::HarnessNotInstalled { install: install.clone() },
            Some(AgentSlot::HarnessUnknown) => Agent::HarnessUnknown,
        };
        Task {
            id: row.id.clone(),
            workspace: row.workspace.clone(),
            folder: row.folder.clone(),
            mode: row.mode,
            directory: row.directory.clone(),
            branch: row.worktree.as_ref().map(|worktree| worktree.branch.clone()),
            parent: row.parent.clone(),
            title: row.title.clone(),
            state: row.state,
            harness: row.harness.clone(),
            session: row.session.clone(),
            ended_at: row.ended_at,
            archived_at: row.archived_at,
            rev: row.rev,
            agent,
            panels: self.panels(&row.id),
            browsers: self.browsers_of(&row.id).to_vec(),
        }
    }
}

impl Server {
    /// A worktree or copy is made outside the state lock, since a copy clones its files.
    pub(crate) fn task_create(&self, op: TaskCreate) -> Result<TaskCreated, Error> {
        nonempty("a task title", &op.title)?;
        let session = match op.session {
            SessionStart::Fresh => None,
            SessionStart::Resume(id) => {
                nonempty("a session id", &id.0)?;
                Some(id)
            }
        };
        let folder = folders::directory("folder", &op.folder)?;
        let filed = |state: &State| {
            let workspace = state.store.workspace(&op.workspace)?;
            match &op.parent {
                Some(parent) if state.store.task(parent)?.workspace != workspace.id => {
                    Err(Error::conflict(format!("{parent} is in another workspace than {}", workspace.name)))
                }
                _ => Ok(()),
            }
        };
        filed(&lock(&self.state))?;
        let (directory, worktree) = match op.mode {
            Mode::InPlace => (PathBuf::from(&folder), None),
            Mode::Worktree | Mode::Copy => {
                let held = |path: &str| lock(&self.state).store.holds_worktree(path);
                let (worktree, directory) =
                    worktrees::branch_off(Path::new(&folder), &self.worktrees, &op.title, op.mode, held)?;
                (directory, Some(worktree))
            }
        };
        let mut state = lock(&self.state);
        let created = filed(&state).and_then(|()| {
            state.store.create_task(TaskRow {
                id: TaskId::random(),
                workspace: op.workspace,
                parent: op.parent,
                title: op.title,
                state: op.state,
                folder: folder.clone(),
                mode: op.mode,
                directory: directory.to_string_lossy().into_owned(),
                worktree: worktree.clone(),
                harness: op.harness,
                session,
                ended_at: ended_at(op.state, now()),
                archived_at: None,
                rev: 0,
            })
        });
        let row = created.inspect_err(|_| {
            if let Some(worktree) = &worktree
                && let Err(undo) = worktrees::discard(Path::new(&folder), worktree)
            {
                report!("remove the worktree {} of a task never created: {undo}", worktree.path);
            }
        })?;
        self.publish_task(&state, &row);
        Ok(TaskCreated { task: row.id, rev: row.rev })
    }

    pub(crate) fn task_move(&self, op: TaskMove) -> Result<Revised, Error> {
        let reopens = |row: &TaskRow| !row.state.is_live() && op.state.is_live();
        let (mut state, mut row) = self.ready_task(&op.task, reopens)?;
        let reopening = reopens(&row);
        let stops = row.state.is_live() && !op.state.is_live();
        row.state = op.state;
        row.ended_at = ended_at(op.state, now());
        row.archived_at = None;
        let rev = state.store.save_task(&mut row)?;
        if reopening {
            state.forget_agent(&row.id);
        }
        if stops {
            state.watchers.remove(&row.id);
            for (name, terminal) in state.terminals(&row.id) {
                if let Err(error) = terminal.terminate() {
                    report!("stop the {name} panel of {}: {error}", row.id);
                }
            }
        }
        self.publish_task(&state, &row);
        Ok(Revised { rev })
    }

    pub(crate) fn task_set_parent(&self, op: TaskSetParent) -> Result<Revised, Error> {
        let mut state = lock(&self.state);
        let mut row = state.store.task(&op.task)?;
        if let Some(parent) = &op.parent {
            let mut above = state.store.task(parent)?;
            if above.workspace != row.workspace {
                return Err(Error::conflict(format!("{parent} is in another workspace than {}", row.id)));
            }
            loop {
                if above.id == row.id {
                    return Err(Error::conflict(format!("{} cannot nest under itself or its own subtask", row.id)));
                }
                let Some(next) = &above.parent else { break };
                above = state.store.task(next)?;
            }
        }
        row.parent = op.parent;
        self.save_and_publish(&mut state, &mut row)
    }

    pub(crate) fn task_set_workspace(&self, op: TaskSetWorkspace) -> Result<Revised, Error> {
        let mut state = lock(&self.state);
        let row = state.store.task(&op.task)?;
        state.store.workspace(&op.workspace)?;
        if let Some(parent) = &row.parent {
            return Err(Error::conflict(format!("{} is a subtask of {parent}, which stays in its workspace", row.id)));
        }
        let refiled = state.store.refile(&row.id, &op.workspace)?;
        for row in &refiled {
            self.publish_task(&state, row);
        }
        let rev = refiled.last().expect("the tree holds the task itself").rev;
        Ok(Revised { rev })
    }

    pub(crate) fn task_rename(&self, op: TaskRename) -> Result<Revised, Error> {
        nonempty("a task title", &op.title)?;
        let mut state = lock(&self.state);
        let mut row = state.store.task(&op.task)?;
        row.title = op.title;
        self.save_and_publish(&mut state, &mut row)
    }

    pub(crate) fn task_set_session(&self, op: TaskSetSession) -> Result<Revised, Error> {
        nonempty("a session id", &op.session.0)?;
        if op.session.0 == FRESH_SESSION {
            return Err(Error::invalid(format!("{FRESH_SESSION} names no existing session")));
        }
        let mut state = lock(&self.state);
        let mut row = state.store.task(&op.task)?;
        row.session = Some(op.session);
        let rev = state.store.save_task(&mut row)?;
        state.forget_agent(&row.id);
        self.publish_task(&state, &row);
        Ok(Revised { rev })
    }

    pub(crate) fn task_archive(&self, op: TaskArchive) -> Result<Revised, Error> {
        let mut state = lock(&self.state);
        let mut row = state.store.task(&op.task)?;
        if row.state.is_live() {
            return Err(Error::conflict(format!("{} is live; end it before archiving it", row.id)));
        }
        row.archived_at = Some(now());
        self.save_and_publish(&mut state, &mut row)
    }
}
