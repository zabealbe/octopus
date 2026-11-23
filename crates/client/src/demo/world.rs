//! What a demo computer holds, a workspace of invented tasks with their terminals, browser panels and files, or on the
//! blank one the project alone, and every op a client sends it, answered as a server answers it and pushed to every
//! client attached.

use std::collections::{BTreeMap, HashMap};

use octopus_proto::account::Account;
use octopus_proto::device::PublicKey;
use octopus_proto::ops::{
    Accounts, BrowserClose, BrowserMove, BrowserOpen, BrowserReport, Done, Entries, FileCopy, FileCreate, FileDelete,
    FileList, FileMove, FileSearch, FileWrite, Folder, FolderGet, Hosted, Matches, Opened, PairedAccount, PairedClient,
    Revised, Shown, Snapshot, TaskArchive, TaskCreate, TaskCreated, TaskFocus, TaskGet, TaskMove, TaskRename,
    TaskSetParent, TaskSetSession, TaskSetWorkspace, TermClaim, TermClose, TermOpen, TermResize, TermSend,
    TermTakeLock, WorkspaceAdd, WorkspaceCreated, WorkspaceRemove, WorkspaceRename, WorkspaceSetColor, Workspaces,
    Written,
};
use octopus_proto::term::Piece;
use octopus_proto::types::{
    AGENT_PANEL, Agent, AgentStatus, BrowserId, BrowserPanel, BrowserTarget, Color, Listening, LockHolder, Mode, Panel,
    PtyId, SessionId, SessionStart, Task, TaskId, TaskState, Workspace, WorkspaceId, web_page,
};
use octopus_proto::{Error, Event, Push, now};
use tokio::sync::{broadcast, oneshot};

use super::project::{Project, ROOT};
use super::sessions::{self, Beat, PROMPT};
use super::terminal::{Program, Term, Turn, Typed};
use super::{Computer, NAME, RELAY, web};

const PUSH_BACKLOG: usize = 1024;
const COLS: u16 = 80;
const ROWS: u16 = 24;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// Another device of the demo's own story, holding one terminal's lock so a client can take it over.
const LAPTOP: &str = "studio-laptop";
const LAPTOP_KEY: &str = "ed25519:df361f8fe4b1000bef699e6cc0bf5cb708d979d9799ebaf3aa092a3efd064861";
const WORKTREES: &str = "/work/worktrees";

/// The client a request or stream comes from: the key its connection proved and the name it gave.
#[derive(Clone)]
pub(super) struct Caller {
    pub key: PublicKey,
    pub name: String,
}

impl Caller {
    fn holder(&self) -> LockHolder {
        LockHolder { name: self.name.clone(), key: Some(self.key) }
    }
}

pub(super) enum Replay {
    Once(PtyId, &'static [Beat]),
    Cycle(PtyId, &'static [&'static [Beat]]),
}

pub(super) struct World {
    computer: Computer,
    rev: u64,
    next_id: u64,
    workspaces: Vec<Workspace>,
    tasks: BTreeMap<TaskId, Task>,
    terms: HashMap<PtyId, Term>,
    project: Project,
    /// The account of the clients attached, as their last `account.sync` gave it.
    account: Option<Account>,
    pushes: broadcast::Sender<Push>,
}

/// The upload a demo task takes at most, since the demo keeps its files in memory.
const UPLOAD_BYTES: u64 = 8 << 20;

fn panel(id: &str, name: &str, lock_holder: Option<LockHolder>) -> Panel {
    let (id, name) = (PtyId(id.into()), name.into());
    Panel { id, name, running: true, lock_holder, cols: COLS, rows: ROWS, listening: Listening::No }
}

fn browser(id: &str, url: &str, title: &str) -> BrowserPanel {
    BrowserPanel { panel: BrowserId(id.into()), url: url.into(), title: title.into(), load: 1 }
}

fn slug(title: &str) -> String {
    let lower = title.to_lowercase();
    let words = lower.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).take(4).collect::<Vec<_>>();
    if words.is_empty() { "task".into() } else { words.join("-") }
}

impl World {
    /// The computer's world and the sessions it replays from the start: the blank one holds the project alone.
    pub(super) fn new(computer: Computer) -> (Self, Vec<Replay>) {
        let (pushes, _) = broadcast::channel(PUSH_BACKLOG);
        let world = Self {
            computer,
            rev: 1,
            next_id: 1,
            workspaces: Vec::new(),
            tasks: BTreeMap::new(),
            terms: HashMap::new(),
            project: Project::new(),
            account: None,
            pushes,
        };
        match computer {
            Computer::Blank => (world, Vec::new()),
            Computer::Tidepool => world.seeded(),
        }
    }

    /// Tidepool's workspace of tasks at work, and the sessions its agents replay.
    fn seeded(mut self) -> (Self, Vec<Replay>) {
        self.workspaces.push(Workspace {
            id: WorkspaceId("wsp_tide0001".into()),
            name: super::PROJECT.into(),
            root: "/work".into(),
            color: Color::parse("teal-vivid").expect("a swatch of the matrix"),
        });
        let laptop = LockHolder { name: LAPTOP.into(), key: Some(LAPTOP_KEY.parse().expect("a point on the curve")) };
        let working = Agent::Running { status: AgentStatus::Idle };
        self.add_task("tsk_tide0001", "Add a weekly tide chart", TaskState::Pinned, None, Some("weekly-tide-chart"));
        self.add_task("tsk_tide0002", "Pick the chart's colors", TaskState::Temporary, Some("tsk_tide0001"), None);
        self.add_task(
            "tsk_tide0003",
            "Fix the hour offset in harbor times",
            TaskState::Temporary,
            None,
            Some("harbor-time-offset"),
        );
        self.add_task(
            "tsk_tide0004",
            "Write tests for the forecast parser",
            TaskState::Temporary,
            None,
            Some("forecast-tests"),
        );
        self.add_task("tsk_tide0005", "Tidy the README", TaskState::Completed, None, None);
        self.edit("tsk_tide0001", |task| {
            task.agent = working.clone();
            task.panels = vec![panel("pty_tide0001", AGENT_PANEL, None)];
            task.browsers = vec![browser("brw_tide0001", &web::page("/"), "tidewatch · Saltmarsh Cove")];
        });
        self.edit("tsk_tide0003", |task| {
            task.agent = working.clone();
            task.panels = vec![panel("pty_tide0003", AGENT_PANEL, Some(laptop))];
        });
        self.edit("tsk_tide0004", |task| {
            task.agent = working.clone();
            task.panels = vec![panel("pty_tide0004", AGENT_PANEL, None), panel("pty_tide0005", "sh", None)];
            task.browsers = vec![browser("brw_tide0002", &web::page("/tests"), "tidewatch · tests")];
        });
        self.edit("tsk_tide0005", |task| task.ended_at = Some(now() - 2 * DAY_MS));
        for (pty, task, program, output) in [
            ("pty_tide0001", "tsk_tide0001", Program::Agent(Turn::Playing), String::new()),
            ("pty_tide0003", "tsk_tide0003", Program::Agent(Turn::Playing), String::new()),
            ("pty_tide0004", "tsk_tide0004", Program::Agent(Turn::Ready), sessions::transcript(sessions::TESTS)),
        ] {
            self.terms.insert(PtyId(pty.into()), Term::new(TaskId(task.into()), program, &output));
        }
        self.terms
            .insert(PtyId("pty_tide0005".into()), Term::shell(TaskId("tsk_tide0004".into()), "forecast-tests".into()));
        let replays = vec![
            Replay::Cycle(PtyId("pty_tide0001".into()), sessions::CHART_TASK),
            Replay::Once(PtyId("pty_tide0003".into()), sessions::OFFSET),
        ];
        (self, replays)
    }

    fn add_task(&mut self, id: &str, title: &str, state: TaskState, parent: Option<&str>, branch: Option<&str>) {
        let workspace = self.workspaces[0].id.clone();
        let (mode, directory) = match branch {
            Some(branch) => (Mode::Worktree, format!("{WORKTREES}/{branch}")),
            None => (Mode::InPlace, ROOT.to_string()),
        };
        let task = Task {
            id: TaskId(id.into()),
            workspace,
            folder: ROOT.into(),
            mode,
            directory,
            branch: branch.map(Into::into),
            parent: parent.map(|parent| TaskId(parent.into())),
            title: title.into(),
            state,
            harness: "agent".into(),
            session: Some(SessionId(format!("demo-{id}"))),
            ended_at: None,
            archived_at: None,
            rev: self.rev,
            agent: Agent::NotStarted,
            panels: Vec::new(),
            browsers: Vec::new(),
        };
        self.tasks.insert(task.id.clone(), task);
    }

    fn edit(&mut self, id: &str, change: impl FnOnce(&mut Task)) {
        change(self.tasks.get_mut(&TaskId(id.into())).expect("the demo's own task"));
    }

    pub(super) fn pushes(&self) -> broadcast::Receiver<Push> {
        self.pushes.subscribe()
    }

    fn id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}demo{:04}", self.next_id)
    }

    fn push(&self, event: Event) {
        let _nobody_attached = self.pushes.send(Push { rev: self.rev, event });
    }

    fn bump(&mut self, event: Event) -> Revised {
        self.rev += 1;
        self.push(event);
        Revised { rev: self.rev }
    }

    fn touch(&mut self, task: &TaskId) -> Revised {
        let Some(task) = self.tasks.get_mut(task) else { return Revised { rev: self.rev } };
        task.rev = self.rev + 1;
        let task = Box::new(task.clone());
        self.bump(Event::TaskUpdated { task })
    }

    fn unchanged(&self) -> Revised {
        Revised { rev: self.rev }
    }

    fn task(&self, id: &TaskId) -> Result<&Task, Error> {
        self.tasks.get(id).ok_or_else(|| Error::not_found(format!("task {id}")))
    }

    fn task_mut(&mut self, id: &TaskId) -> Result<&mut Task, Error> {
        self.tasks.get_mut(id).ok_or_else(|| Error::not_found(format!("task {id}")))
    }

    fn workspace(&mut self, id: &WorkspaceId) -> Result<&mut Workspace, Error> {
        self.workspaces
            .iter_mut()
            .find(|workspace| workspace.id == *id)
            .ok_or_else(|| Error::not_found(format!("workspace {id}")))
    }

    fn panel(&mut self, task: &TaskId, panel: &str) -> Result<&mut Panel, Error> {
        let panels = &mut self.task_mut(task)?.panels;
        panels
            .iter_mut()
            .find(|known| known.answers_to(panel))
            .ok_or_else(|| Error::not_found(format!("panel {panel} of {task}")))
    }

    pub(super) fn rev(&self) -> u64 {
        self.rev
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot { rev: self.rev, workspaces: self.workspaces.clone(), tasks: self.tasks.values().cloned().collect() }
    }

    pub(super) fn hosted(&self) -> Hosted {
        Hosted { key: self.computer.key(), relay: RELAY.clone(), name: NAME.into() }
    }

    pub(super) fn keep_account(&mut self, account: &Account) {
        self.account = Some(account.clone());
    }

    pub(super) fn accounts(&self, caller: &Caller) -> Accounts {
        let accounts = self.account.iter().map(|account| PairedAccount {
            account: account.id(),
            devices: account
                .state()
                .members
                .into_iter()
                .map(|member| PairedClient {
                    connected: member.key == caller.key,
                    seen_at: (member.key == caller.key).then(now),
                    name: member.name,
                    key: member.key,
                    hosts: false,
                })
                .collect(),
        });
        Accounts { accounts: accounts.collect() }
    }

    pub(super) fn workspace_add(&mut self, op: WorkspaceAdd) -> Result<WorkspaceCreated, Error> {
        if self.workspaces.iter().any(|workspace| workspace.name == op.name) {
            return Err(Error::conflict(format!("a workspace named {} exists", op.name)));
        }
        let workspace = Workspace { id: WorkspaceId(self.id("wsp_")), name: op.name, root: op.root, color: op.color };
        self.workspaces.push(workspace.clone());
        let Revised { rev } = self.bump(Event::WorkspaceUpdated { workspace: workspace.clone() });
        Ok(WorkspaceCreated { workspace: workspace.id, rev })
    }

    pub(super) fn workspace_list(&self) -> Workspaces {
        Workspaces { workspaces: self.workspaces.clone() }
    }

    fn workspace_changed(&mut self, id: &WorkspaceId, change: impl FnOnce(&mut Workspace)) -> Result<Revised, Error> {
        let workspace = self.workspace(id)?;
        change(workspace);
        let workspace = workspace.clone();
        Ok(self.bump(Event::WorkspaceUpdated { workspace }))
    }

    pub(super) fn workspace_rename(&mut self, op: WorkspaceRename) -> Result<Revised, Error> {
        if self.workspaces.iter().any(|workspace| workspace.name == op.name && workspace.id != op.workspace) {
            return Err(Error::conflict(format!("a workspace named {} exists", op.name)));
        }
        self.workspace_changed(&op.workspace, |workspace| workspace.name = op.name)
    }

    pub(super) fn workspace_set_color(&mut self, op: WorkspaceSetColor) -> Result<Revised, Error> {
        self.workspace_changed(&op.workspace, |workspace| workspace.color = op.color)
    }

    pub(super) fn workspace_remove(&mut self, op: WorkspaceRemove) -> Result<Revised, Error> {
        self.workspace(&op.workspace)?;
        let tasks = self.tasks.values().filter(|task| task.workspace == op.workspace);
        if tasks.clone().any(|task| task.state.is_live()) {
            return Err(Error::conflict("the workspace has live tasks"));
        }
        let gone: Vec<TaskId> = tasks.map(|task| task.id.clone()).collect();
        self.terms.retain(|_, term| !gone.contains(&term.task));
        self.tasks.retain(|id, _| !gone.contains(id));
        self.workspaces.retain(|workspace| workspace.id != op.workspace);
        Ok(self.bump(Event::WorkspaceRemoved { workspace: op.workspace }))
    }

    pub(super) fn folder_get(&self, op: FolderGet) -> Result<Folder, Error> {
        self.project.folder(&op.folder)
    }

    pub(super) fn harness_set(&mut self, id: String) -> Revised {
        self.bump(Event::HarnessUpdated { harness: id })
    }

    pub(super) fn task_create(&mut self, op: TaskCreate) -> Result<TaskCreated, Error> {
        self.workspace(&op.workspace)?;
        if op.title.trim().is_empty() {
            return Err(Error::invalid("a task has a title"));
        }
        self.project.folder(&op.folder)?;
        if let Some(parent) = &op.parent
            && self.task(parent)?.workspace != op.workspace
        {
            return Err(Error::invalid("a subtask sits in its parent's workspace"));
        }
        let id = self.id("tsk_");
        let branch = (op.mode != Mode::InPlace).then(|| slug(&op.title));
        self.add_task(&id, &op.title, op.state, None, branch.as_deref());
        let session = match op.session {
            SessionStart::Fresh => None,
            SessionStart::Resume(id) => Some(id),
        };
        let task = TaskId(id);
        let created = self.task_mut(&task)?;
        if op.mode == Mode::InPlace {
            created.directory.clone_from(&op.folder);
        }
        (created.workspace, created.folder, created.parent, created.harness) =
            (op.workspace, op.folder, op.parent, op.harness);
        created.mode = op.mode;
        created.session = session;
        if !op.state.is_live() {
            created.ended_at = Some(now());
        }
        let Revised { rev } = self.touch(&task);
        Ok(TaskCreated { task, rev })
    }

    pub(super) fn task_get(&self, op: TaskGet) -> Result<Task, Error> {
        self.task(&op.task).cloned()
    }

    fn task_changed(&mut self, id: &TaskId, change: impl FnOnce(&mut Task)) -> Result<Revised, Error> {
        change(self.task_mut(id)?);
        Ok(self.touch(id))
    }

    pub(super) fn task_set_session(&mut self, op: TaskSetSession) -> Result<Revised, Error> {
        self.task_changed(&op.task, |task| task.session = Some(op.session))
    }

    pub(super) fn task_move(&mut self, op: TaskMove) -> Result<Revised, Error> {
        self.task_changed(&op.task, |task| {
            task.ended_at = if op.state.is_live() { None } else { task.ended_at.or(Some(now())) };
            task.archived_at = None;
            task.state = op.state;
        })
    }

    pub(super) fn task_set_parent(&mut self, op: TaskSetParent) -> Result<Revised, Error> {
        let workspace = self.task(&op.task)?.workspace.clone();
        let mut above = op.parent.clone();
        while let Some(parent) = above {
            let parent = self.task(&parent)?;
            if parent.id == op.task || parent.workspace != workspace {
                return Err(Error::invalid("a parent is another task of the same workspace, not under this one"));
            }
            above = parent.parent.clone();
        }
        self.task_changed(&op.task, |task| task.parent = op.parent)
    }

    pub(super) fn task_set_workspace(&mut self, op: TaskSetWorkspace) -> Result<Revised, Error> {
        self.workspace(&op.workspace)?;
        if self.task(&op.task)?.parent.is_some() {
            return Err(Error::invalid("only a root task moves to another workspace"));
        }
        let mut moving = vec![op.task.clone()];
        let mut at = 0;
        while let Some(parent) = moving.get(at).cloned() {
            moving.extend(
                self.tasks.values().filter(|task| task.parent.as_ref() == Some(&parent)).map(|task| task.id.clone()),
            );
            at += 1;
        }
        let mut revised = Revised { rev: self.rev };
        for task in moving {
            revised = self.task_changed(&task, |task| task.workspace = op.workspace.clone())?;
        }
        Ok(revised)
    }

    pub(super) fn task_rename(&mut self, op: TaskRename) -> Result<Revised, Error> {
        self.task_changed(&op.task, |task| task.title = op.title)
    }

    pub(super) fn task_archive(&mut self, op: TaskArchive) -> Result<Revised, Error> {
        if self.task(&op.task)?.state.is_live() {
            return Err(Error::conflict("only an ended task is archived"));
        }
        self.task_changed(&op.task, |task| task.archived_at = Some(now()))
    }

    /// Wakes the task's agent on a fresh session; on the blank computer the agent goes on to take a request and work.
    pub(super) fn task_focus(&mut self, op: TaskFocus) -> Result<(Revised, Option<Replay>), Error> {
        let task = self.task(&op.task)?;
        if !task.state.is_live() || matches!(task.agent, Agent::Running { .. }) {
            return Ok((self.unchanged(), None));
        }
        let pty = PtyId(self.id("pty_"));
        let fresh = format!("{}{PROMPT}", sessions::FRESH);
        self.terms.insert(pty.clone(), Term::new(op.task.clone(), Program::Agent(Turn::Ready), &fresh));
        let revised = self.task_changed(&op.task, |task| {
            task.agent = Agent::Running { status: AgentStatus::Idle };
            task.panels.retain(|panel| panel.name != AGENT_PANEL);
            task.panels.insert(0, panel(&pty.0, AGENT_PANEL, None));
        })?;
        Ok((revised, self.computer.waking().map(|beats| Replay::Once(pty, beats))))
    }

    pub(super) fn term_open(&mut self, op: TermOpen) -> Result<Opened, Error> {
        let task = self.task(&op.task)?;
        if !task.state.is_live() {
            return Err(Error::conflict("an ended task opens no shell"));
        }
        self.project.list(&op.path)?;
        let taken = |name: &str| task.panels.iter().any(|panel| panel.name == name);
        let name = (1..).map(|n| if n == 1 { "sh".to_string() } else { format!("sh-{n}") }).find(|name| !taken(name));
        let name = name.expect("some name is free");
        let branch = task.branch.clone().unwrap_or_else(|| "main".into());
        let pty = PtyId(self.id("pty_"));
        self.terms.insert(pty.clone(), Term::shell(op.task.clone(), branch));
        let Revised { rev } = self.task_changed(&op.task, |task| task.panels.push(panel(&pty.0, &name, None)))?;
        Ok(Opened { panel: pty, rev })
    }

    pub(super) fn term_close(&mut self, op: TermClose) -> Result<Revised, Error> {
        let panel = self.panel(&op.task, &op.panel)?;
        if panel.name == AGENT_PANEL {
            return Err(Error::conflict("the agent panel stays open"));
        }
        let pty = panel.id.clone();
        Ok(self.close(&pty))
    }

    /// Drops the terminal and its panel, which ends every stream showing it.
    pub(super) fn close(&mut self, pty: &PtyId) -> Revised {
        let Some(term) = self.terms.remove(pty) else { return self.unchanged() };
        if let Some(task) = self.tasks.get_mut(&term.task) {
            task.panels.retain(|panel| panel.id != *pty);
        }
        self.touch(&term.task)
    }

    fn resize(
        &mut self,
        task: &TaskId,
        panel: &str,
        cols: u16,
        rows: u16,
        holder: Option<LockHolder>,
    ) -> Result<Revised, Error> {
        let panel = self.panel(task, panel)?;
        let resized = (panel.cols, panel.rows) != (cols, rows);
        let relocked = holder.is_some() && panel.lock_holder != holder;
        (panel.cols, panel.rows) = (cols, rows);
        if relocked {
            panel.lock_holder = holder;
        }
        let pty = panel.id.clone();
        if resized && let Some(term) = self.terms.get(&pty) {
            term.resized(cols, rows);
        }
        Ok(if resized || relocked { self.touch(task) } else { self.unchanged() })
    }

    pub(super) fn term_claim(&mut self, caller: &Caller, op: TermClaim) -> Result<Revised, Error> {
        let panel = self.panel(&op.task, &op.panel)?;
        if let Some(holder) = &panel.lock_holder
            && holder.key != Some(caller.key)
        {
            return Err(Error::PtyLocked { holder: holder.name.clone() });
        }
        self.resize(&op.task, &op.panel, op.cols, op.rows, Some(caller.holder()))
    }

    pub(super) fn term_take_lock(&mut self, caller: &Caller, op: TermTakeLock) -> Result<Revised, Error> {
        self.resize(&op.task, &op.panel, op.cols, op.rows, Some(caller.holder()))
    }

    pub(super) fn term_resize(&mut self, caller: &Caller, op: TermResize) -> Result<Revised, Error> {
        if self.panel(&op.task, &op.panel)?.lock_holder.as_ref().and_then(|holder| holder.key) != Some(caller.key) {
            return Err(Error::conflict("only the terminal's lock holder resizes it"));
        }
        self.resize(&op.task, &op.panel, op.cols, op.rows, None)
    }

    pub(super) fn term_send(&mut self, caller: &Caller, op: TermSend) -> Result<(Revised, PtyId, Vec<Typed>), Error> {
        let panel = self.panel(&op.task, &op.panel)?;
        let relocked = op.take_lock && panel.lock_holder != Some(caller.holder());
        if relocked {
            panel.lock_holder = Some(caller.holder());
        }
        match &panel.lock_holder {
            Some(holder) if holder.key != Some(caller.key) => {
                return Err(Error::PtyLocked { holder: holder.name.clone() });
            }
            None => return Err(Error::conflict("the terminal's lock is free; take it first")),
            Some(_) => {}
        }
        let pty = panel.id.clone();
        let revised = if relocked { self.touch(&op.task) } else { self.unchanged() };
        let text = if op.enter { format!("{}\r", op.text) } else { op.text };
        let typed = self.type_into(&pty, &caller.key, &text);
        Ok((revised, pty, typed))
    }

    pub(super) fn attach(
        &mut self,
        pty: &PtyId,
        viewer: PublicKey,
    ) -> Result<(Piece, broadcast::Receiver<Piece>), Error> {
        let (cols, rows) = self.size(pty).ok_or_else(|| Error::not_found(format!("terminal {pty}")))?;
        let term = self.terms.get_mut(pty).ok_or_else(|| Error::not_found(format!("terminal {pty}")))?;
        Ok(term.attach(viewer, cols, rows))
    }

    fn size(&self, pty: &PtyId) -> Option<(u16, u16)> {
        let panels = self.tasks.values().flat_map(|task| &task.panels);
        panels.filter(|panel| panel.id == *pty).map(|panel| (panel.cols, panel.rows)).next()
    }

    /// A client stops showing the terminal on one stream; its lock frees once it shows it on none.
    pub(super) fn detach(&mut self, pty: &PtyId, viewer: PublicKey) {
        let Some(term) = self.terms.get_mut(pty) else { return };
        if term.detach(viewer) {
            return;
        }
        let task = term.task.clone();
        let Ok(panel) = self.panel(&task, &pty.0) else { return };
        if panel.lock_holder.as_ref().is_some_and(|holder| holder.key == Some(viewer)) {
            panel.lock_holder = None;
            self.touch(&task);
        }
    }

    pub(super) fn type_into(&mut self, pty: &PtyId, typist: &PublicKey, text: &str) -> Vec<Typed> {
        let holds = self.tasks.values().flat_map(|task| &task.panels).any(|panel| {
            panel.id == *pty && panel.lock_holder.as_ref().is_some_and(|holder| holder.key == Some(*typist))
        });
        match self.terms.get_mut(pty) {
            Some(term) if holds => term.type_text(text, &self.project),
            _ => Vec::new(),
        }
    }

    /// Resolves to whether the terminal is still there.
    pub(super) fn say(&mut self, pty: &PtyId, text: &str) -> bool {
        self.terms.get_mut(pty).map(|term| term.write(text)).is_some()
    }

    pub(super) fn clear(&mut self, pty: &PtyId) -> bool {
        self.terms.get_mut(pty).map(Term::clear).is_some()
    }

    pub(super) fn turn(&mut self, pty: &PtyId, next: Turn) {
        if let Some(Term { program: Program::Agent(turn), .. }) = self.terms.get_mut(pty) {
            *turn = next;
        }
    }

    pub(super) fn stage(&self, pty: &PtyId) -> Option<std::sync::Arc<tokio::sync::Mutex<()>>> {
        self.terms.get(pty).map(|term| std::sync::Arc::clone(&term.stage))
    }

    /// The next line typed into the agent's terminal.
    pub(super) fn ask(&mut self, pty: &PtyId) -> Option<oneshot::Receiver<String>> {
        let (answer, answered) = oneshot::channel();
        match self.terms.get_mut(pty) {
            Some(Term { program: Program::Agent(turn), .. }) => {
                *turn = Turn::Asking(answer);
                Some(answered)
            }
            _ => None,
        }
    }

    pub(super) fn status(&mut self, pty: &PtyId, status: AgentStatus) -> bool {
        let Some(task) = self.terms.get(pty).map(|term| term.task.clone()) else { return false };
        self.task_changed(&task, |task| task.agent = Agent::Running { status }).is_ok()
    }

    pub(super) fn file(&self, task: &TaskId, path: &str) -> Result<&[u8], Error> {
        self.task(task)?;
        self.project.file(path)
    }

    pub(super) fn file_list(&self, op: FileList) -> Result<Entries, Error> {
        self.task(&op.task)?;
        Ok(Entries { entries: self.project.list(&op.path)? })
    }

    fn changed(&self, path: &str) {
        for task in self.tasks.keys() {
            self.push(Event::FileChanged { task: task.clone(), path: path.into() });
        }
    }

    pub(super) fn file_write(&mut self, op: FileWrite) -> Result<Written, Error> {
        self.task(&op.task)?;
        let bytes = op.content.len() as u64;
        let path = self.project.write(&op.path, op.content.into_bytes())?;
        self.changed(&path);
        Ok(Written { bytes })
    }

    pub(super) fn may_upload(&self, task: &TaskId, bytes: u64) -> Result<(), Error> {
        self.task(task)?;
        if bytes > UPLOAD_BYTES {
            return Err(Error::invalid(format!("the demo takes uploads of {UPLOAD_BYTES} bytes at most")));
        }
        Ok(())
    }

    pub(super) fn upload(&mut self, task: &TaskId, path: &str, content: Vec<u8>) -> Result<Done, Error> {
        self.task(task)?;
        let path = self.project.write(path, content)?;
        self.changed(&path);
        Ok(Done {})
    }

    pub(super) fn file_create(&mut self, op: FileCreate) -> Result<Done, Error> {
        self.task(&op.task)?;
        let path = self.project.create(&op.path, op.kind)?;
        self.changed(&path);
        Ok(Done {})
    }

    fn file_copy_or_move(&mut self, task: &TaskId, from: &str, to: &str, moving: bool) -> Result<Done, Error> {
        self.task(task)?;
        let to = self.project.copy(from, to, moving)?;
        self.changed(&to);
        if moving {
            self.changed(from);
        }
        Ok(Done {})
    }

    pub(super) fn file_move(&mut self, op: FileMove) -> Result<Done, Error> {
        self.file_copy_or_move(&op.task, &op.from, &op.to, true)
    }

    pub(super) fn file_copy(&mut self, op: FileCopy) -> Result<Done, Error> {
        self.file_copy_or_move(&op.task, &op.from, &op.to, false)
    }

    pub(super) fn file_delete(&mut self, op: FileDelete) -> Result<Done, Error> {
        self.task(&op.task)?;
        let path = self.project.delete(&op.path)?;
        self.changed(&path);
        Ok(Done {})
    }

    pub(super) fn file_search(&self, op: FileSearch) -> Result<Matches, Error> {
        self.task(&op.task)?;
        Ok(Matches { matches: self.project.search(&op.pattern) })
    }

    pub(super) fn browser_open(&mut self, op: BrowserOpen) -> Result<Shown, Error> {
        web_page(&op.url).map_err(Error::invalid)?;
        let id = BrowserId(self.id("brw_"));
        let load = u64::from(op.panel == BrowserTarget::New);
        let task = self.task_mut(&op.task)?;
        let panel = match op.panel {
            BrowserTarget::New | BrowserTarget::Background => {
                task.browsers.push(BrowserPanel { panel: id.clone(), url: op.url, title: String::new(), load });
                id
            }
            BrowserTarget::Panel(id) => {
                let shown = task
                    .browsers
                    .iter_mut()
                    .find(|shown| shown.panel == id)
                    .ok_or_else(|| Error::not_found(format!("browser panel {id}")))?;
                (shown.url, shown.title, shown.load) = (op.url, String::new(), shown.load + 1);
                id
            }
        };
        let Revised { rev } = self.touch(&op.task);
        Ok(Shown { panel, rev })
    }

    fn browser_mut(&mut self, task: &TaskId, panel: &BrowserId) -> Result<&mut BrowserPanel, Error> {
        let browsers = &mut self.task_mut(task)?.browsers;
        browsers
            .iter_mut()
            .find(|shown| shown.panel == *panel)
            .ok_or_else(|| Error::not_found(format!("browser panel {panel}")))
    }

    pub(super) fn browser_report(&mut self, op: BrowserReport) -> Result<Revised, Error> {
        let shown = self.browser_mut(&op.task, &op.panel)?;
        if shown.url == op.url && shown.title == op.title {
            return Ok(self.unchanged());
        }
        (shown.url, shown.title) = (op.url, op.title);
        Ok(self.touch(&op.task))
    }

    pub(super) fn browser_close(&mut self, op: BrowserClose) -> Result<Revised, Error> {
        self.browser_mut(&op.task, &op.panel)?;
        self.task_changed(&op.task, |task| task.browsers.retain(|shown| shown.panel != op.panel))
    }

    pub(super) fn browser_move(&mut self, op: BrowserMove) -> Result<Revised, Error> {
        let browsers = &mut self.task_mut(&op.task)?.browsers;
        let from = browsers
            .iter()
            .position(|shown| shown.panel == op.panel)
            .ok_or_else(|| Error::not_found(format!("browser panel {}", op.panel)))?;
        let to = usize::from(op.to);
        if to >= browsers.len() {
            return Err(Error::invalid(format!("{to} is past the last browser panel")));
        }
        if from == to {
            return Ok(self.unchanged());
        }
        let shown = browsers.remove(from);
        browsers.insert(to, shown);
        Ok(self.touch(&op.task))
    }
}
