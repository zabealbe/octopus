use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::account::{Account, AccountId, Signed};
use crate::device::PublicKey;
use crate::frames::Op;
use crate::pairing::{Invite, Proof};
use crate::remote::RelayUrl;
use crate::schema::{Field, record};
use crate::settings::Settings;
use crate::types::{
    BrowserId, BrowserTarget, Color, Mode, PtyId, SessionId, SessionStart, Task, TaskId, TaskState, Workspace,
    WorkspaceId,
};

/// Declares an op: its args as a `record!`, its wire name, and the type of its result.
macro_rules! op {
    ($(#[doc = $doc:literal])* $name:ident = $wire:literal -> $ok:ty { $($fields:tt)* }) => {
        record!($(#[doc = $doc])* $name { $($fields)* });

        impl Op for $name {
            const NAME: &'static str = $wire;
            const DOC: &'static str = concat!($($doc),*);
            type Ok = $ok;
        }
    };
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revised {
    pub rev: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Welcome {
    pub server: String,
    pub rev: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceCreated {
    pub workspace: WorkspaceId,
    pub rev: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCreated {
    pub task: TaskId,
    pub rev: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspaces {
    pub workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub rev: u64,
    pub workspaces: Vec<Workspace>,
    pub tasks: Vec<Task>,
}

op!(
    /// Open a session; the first request on every control connection.
    SessionHello = "session.hello" -> Welcome {
        /// This client's name, as terminal lock holders see it.
        client: String,
    }
);
op!(
    /// Answer with the server's whole state.
    SessionSync = "session.sync" -> Snapshot {
        /// The rev of the client's cache of this server, or null for a client holding none.
        since_rev: Option<u64>,
    }
);
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Done {}

op!(
    /// Answer at once; the client times its connection's round trip with it.
    SessionPing = "session.ping" -> Done {}
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Version {
    /// The version of the octopus binary the server runs, as `octopus --version` prints it.
    pub version: String,
}

op!(
    /// Report the version of the octopus binary the server runs.
    ServerVersion = "server.version" -> Version {}
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    #[ts(type = "string")]
    pub invite: Invite,
    /// When the invite stops pairing, in milliseconds since 1970.
    pub expires_at: i64,
}

/// A member device of an account the server trusts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PairedClient {
    /// The name the device last gave, else the one its account gives it.
    pub name: String,
    /// The device's key, which is who it is.
    #[ts(type = "string")]
    pub key: PublicKey,
    /// Whether this server runs on the device's machine, which reaches it over its local socket.
    pub hosts: bool,
    pub connected: bool,
    /// When one of its connections last opened or closed, in milliseconds since 1970; null if it never connected.
    pub seen_at: Option<i64>,
}

/// An account the server trusts, and its member devices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PairedAccount {
    #[ts(type = "string")]
    pub account: AccountId,
    pub devices: Vec<PairedClient>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accounts {
    pub accounts: Vec<PairedAccount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paired {
    /// The server's name, which the client lists it under.
    pub server: String,
    pub proof: Proof,
}

/// The server as a device of the account lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hosted {
    pub key: PublicKey,
    pub relay: RelayUrl,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Synced {
    /// The account merged with the server's log of it.
    pub account: Account,
    /// The account's settings merged with the server's.
    pub settings: Settings,
    pub server: Hosted,
}

op!(
    /// Show a one-time invite that pairs one client with this server.
    PairingOffer = "pairing.offer" -> Offer {}
);
op!(
    /// List the accounts this server trusts and their devices.
    PairingList = "pairing.list" -> Accounts {}
);
op!(
    /// Stop trusting an account, closing its devices' connections and freeing their locks; it pairs again only with a new invite.
    PairingForget = "pairing.forget" -> Done {
        /// The account's id.
        account: AccountId,
    }
);
op!(
    /// Pair with an invite's code: one of the two requests an unpaired device may send, on its connection's first stream.
    PairingPair = "pairing.pair" -> Paired {
        /// The client's name, as terminal lock holders see it.
        client: String,
        /// The client's proof that it holds the code.
        proof: Proof,
        /// The client's account, which the server trusts once the code is proved.
        account: Account,
    }
);
op!(
    /// Merge this device's account, and the accounts it left, into the server's logs, and the account's settings into the server's, and return the merged account and settings.
    AccountSync = "account.sync" -> Synced {
        account: Account,
        /// Each account this device left, so a server that trusted one follows it.
        former: Vec<Account>,
        /// The account's settings as this device holds them.
        settings: Settings,
    }
);
op!(
    /// Join a trusted account with a link's invite: one of the two requests an unpaired device may send.
    AccountJoin = "account.join" -> Synced {
        /// The id of the account the link names.
        account: AccountId,
        /// The device's `join` entry.
        join: Signed,
    }
);

op!(
    /// Add a workspace: a named place this server files tasks in.
    WorkspaceAdd = "workspace.add" -> WorkspaceCreated {
        /// The workspace's name, unique on its server.
        name: String,
        /// Where new tasks' folders start: a directory on the server's machine, as an absolute path.
        root: String,
        /// The workspace's color: auto, or a <hue>-<tone> of the matrix.
        color: Color,
    }
);
op!(
    /// List the server's workspaces.
    WorkspaceList = "workspace.list" -> Workspaces {}
);
op!(
    /// Rename a workspace; it keeps its root and its tasks.
    WorkspaceRename = "workspace.rename" -> Revised {
        /// The workspace.
        workspace: WorkspaceId,
        /// Its new name, unique on its server.
        name: String,
    }
);
op!(
    /// Set a workspace's color; it keeps its name, root and tasks.
    WorkspaceSetColor = "workspace.set_color" -> Revised {
        /// The workspace.
        workspace: WorkspaceId,
        /// Its new color: auto, or a <hue>-<tone> of the matrix.
        color: Color,
    }
);
op!(
    /// Remove a workspace with no live task, with every task in it; their worktrees go and their branches stay.
    WorkspaceRemove = "workspace.remove" -> Revised {
        /// The workspace.
        workspace: WorkspaceId,
    }
);
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    /// The folder, its symlinks, `.` and `..` resolved.
    pub folder: String,
    /// The top of the git repo the folder sits in, or null outside one.
    pub repo: Option<String>,
    /// The branch the repo's HEAD is on, which a worktree branches from, or null outside a repo or on a detached HEAD.
    pub branch: Option<String>,
    /// The folders inside it, or what lets the server in when its OS keeps the server out.
    pub inside: Inside,
}

/// What the server finds inside a folder: the folders it holds, or that the server's OS keeps the server out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Inside {
    /// The folders directly inside it, in name order.
    Listed { folders: Vec<Subfolder> },
    /// The OS setting that lets the server in, or null when the folder's permissions keep the server's user out.
    KeptOut { setting: Option<OsSetting> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Subfolder {
    pub name: String,
    /// Whether the server's OS guards it, so reading it can wait on the OS asking its user.
    pub guarded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct OsSetting {
    /// Where the setting sits, as the OS names it, such as `System Settings › Privacy & Security › Files & Folders`.
    pub name: String,
    /// The URL that opens it.
    pub url: String,
}

op!(
    /// Show a folder on the server's machine, for a new task to pick: the git repo and branch it sits in and the folders
    /// inside it, without looking inside them. Waits for the OS when it guards the folder.
    FolderGet = "folder.get" -> Folder {
        /// The folder, as an absolute path, or ~ for the home folder of the server's user.
        folder: String,
    }
);

/// The ops that can wait on the server's OS asking its user, which a server runs beside a client's queue of ops so the
/// wait holds up none of them.
pub const WAITING_ON_THE_OS: [&str; 1] = [FolderGet::NAME];
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct HarnessStatus {
    /// The program the harness's argv starts.
    pub program: String,
    /// Whether the PATH of the user's login environment holds the program.
    pub installed: bool,
    /// The command that installs the program.
    pub install: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Harnesses {
    /// Every harness id the server knows, sorted.
    pub harnesses: Vec<String>,
}

op!(
    /// List every harness the server knows: its built-in ones and the ones `harness.set` stored.
    HarnessList = "harness.list" -> Harnesses {}
);
op!(
    /// Show whether a harness's program is installed on the server, and the command that installs it.
    HarnessCheck = "harness.check" -> HarnessStatus {
        /// The harness's id, which tasks name.
        id: String,
    }
);
op!(
    /// Declare how a harness starts its agent.
    HarnessSet = "harness.set" -> Revised {
        /// The harness's id, which tasks name.
        id: String,
        /// The agent's argv as a JSON array of strings, e.g. ["claude"].
        argv: Vec<String>,
        /// The args appended to argv to resume the task's session, with {session} for its id, e.g.
        /// ["--resume","{session}"].
        resume: Vec<String>,
        /// The variables the agent's terminal exports beside every task terminal's, e.g.
        /// [{"name":"FORCE_COLOR","value":"1"}]. {files} in argv, resume and a value stands for the server's
        /// harnesses folder.
        env: Vec<EnvVar>,
        /// The command that installs the agent, shown when it is missing.
        install: String,
    }
);
record!(
    /// A variable a harness's agent terminal exports.
    EnvVar {
        /// The variable's name.
        name: String,
        /// Its value.
        value: String,
    }
);
op!(
    /// Create a task, which works in its folder for life.
    TaskCreate = "task.create" -> TaskCreated {
        /// The workspace the task is filed in.
        workspace: WorkspaceId,
        /// The directory on the server's machine the task works in, as an absolute path.
        folder: String,
        /// Where the task runs: in_place in the folder, or worktree or copy in the same folder inside a worktree of
        /// the folder's git repo, a copy with the repo's ignored files cloned in.
        mode: Mode,
        /// The task's title.
        title: String,
        /// The parent task's id, a task of the same workspace.
        parent: Option<TaskId>,
        /// The task's state.
        state: TaskState,
        /// The harness that runs the task's agent.
        harness: String,
        /// An existing session's id for the agent to resume.
        session: SessionStart,
    }
);
op!(
    /// Point a task's agent at an existing session, ending the agent running now; the next focus resumes it.
    TaskSetSession = "task.set_session" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The session to resume.
        session: SessionId,
    }
);
op!(
    /// Record the session a task's agent runs in, as the agent reports it, leaving the agent running; its next start
    /// resumes it.
    AgentReportSession = "agent.report_session" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The session the agent runs in.
        session: SessionId,
    }
);
op!(
    /// Move a task to a state.
    TaskMove = "task.move" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The state to move it to.
        state: TaskState,
    }
);
op!(
    /// Nest a task under another task of its workspace, or make it a root task; it keeps its state and its subtasks.
    TaskSetParent = "task.set_parent" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The parent task's id.
        parent: Option<TaskId>,
    }
);
op!(
    /// File a root task and all its subtasks in another workspace, changing nothing else.
    TaskSetWorkspace = "task.set_workspace" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The workspace to file it in.
        workspace: WorkspaceId,
    }
);
op!(
    /// Rename a task; a worktree task's branch keeps its name.
    TaskRename = "task.rename" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The task's new title.
        title: String,
    }
);
op!(
    /// Archive an ended task, which takes it off the Tasks board until it moves again.
    TaskArchive = "task.archive" -> Revised {
        /// The task's id.
        task: TaskId,
    }
);
op!(
    /// Focus a task, starting its agent if it has none.
    TaskFocus = "task.focus" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The args an agent this focus starts runs after its harness's argv, before its start or resume
        /// args, e.g. ["--model","opus"]; a running agent keeps the args it started with.
        args: Vec<String>,
    }
);
op!(
    /// Print a task.
    TaskGet = "task.get" -> Task {
        /// The task's id.
        task: TaskId,
    }
);
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opened {
    pub panel: PtyId,
    pub rev: u64,
}

op!(
    /// Open a shell panel in a live task: the user's login shell, in a folder of the task's directory.
    TermOpen = "term.open" -> Opened {
        /// The task's id.
        task: TaskId,
        /// The directory the shell starts in, relative to the task's directory; . for the directory itself.
        path: String,
    }
);
op!(
    /// Open a shell panel in a live task that runs `sudo octopus microphone install` on the task's server, where the
    /// user types the server's password; the panel closes when the install succeeds.
    TermInstallMicrophone = "term.install_microphone" -> Opened {
        /// The task's id.
        task: TaskId,
    }
);
op!(
    /// Stop a shell panel and drop it with its scrollback.
    TermClose = "term.close" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The shell panel's name or id.
        panel: String,
    }
);
op!(
    /// Type text into a terminal panel.
    TermSend = "term.send" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The panel's name or id, e.g. agent.
        panel: String,
        /// The text to type.
        text: String,
        /// Whether Enter follows the text.
        enter: bool,
        /// Whether to take the panel's write lock from its holder.
        take_lock: bool,
    }
);
op!(
    /// Take a terminal panel's write lock if it is free and resize it to this client's screen; fails pty_locked
    /// when another client holds it.
    TermClaim = "term.claim" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The panel's name or id, e.g. agent.
        panel: String,
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    }
);
op!(
    /// Take a terminal panel's write lock from its holder and resize it to this client's screen.
    TermTakeLock = "term.take_lock" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The panel's name or id, e.g. agent.
        panel: String,
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    }
);
op!(
    /// Resize a terminal panel; only its lock holder may.
    TermResize = "term.resize" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The panel's name or id, e.g. agent.
        panel: String,
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    }
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
}

impl EntryKind {
    pub const ALL: [Self; 2] = [Self::File, Self::Directory];
}

impl crate::schema::Arg for EntryKind {
    fn shape() -> crate::schema::Shape {
        crate::types::choice(&Self::ALL)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entries {
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Match {
    pub path: String,
    pub line: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Matches {
    pub matches: Vec<Match>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Written {
    pub bytes: u64,
}

op!(
    /// List a folder of a task's directory.
    FileList = "file.list" -> Entries {
        /// The task's id.
        task: TaskId,
        /// The directory, relative to the task's directory; . for the directory itself.
        path: String,
    }
);
op!(
    /// Create a file in a task's directory, or replace its content.
    FileWrite = "file.write" -> Written {
        /// The task's id.
        task: TaskId,
        /// The file, relative to the task's directory.
        path: String,
        /// The file's new content, as text.
        content: String,
    }
);
op!(
    /// Create an empty file or directory in a task's directory, with the directories above it; fails conflict when
    /// the path exists.
    FileCreate = "file.create" -> Done {
        /// The task's id.
        task: TaskId,
        /// The new entry, relative to the task's directory.
        path: String,
        /// What to create.
        kind: EntryKind,
    }
);
op!(
    /// Move or rename a file or directory of a task's directory; fails conflict when the destination exists.
    FileMove = "file.move" -> Done {
        /// The task's id.
        task: TaskId,
        /// The entry, relative to the task's directory.
        from: String,
        /// Its new path, relative to the task's directory.
        to: String,
    }
);
op!(
    /// Copy a file or directory of a task's directory, a directory with everything in it; fails conflict when the
    /// destination exists.
    FileCopy = "file.copy" -> Done {
        /// The task's id.
        task: TaskId,
        /// The entry, relative to the task's directory.
        from: String,
        /// The copy's path, relative to the task's directory.
        to: String,
    }
);
op!(
    /// Delete a file or directory of a task's directory, a directory with everything in it.
    FileDelete = "file.delete" -> Done {
        /// The task's id.
        task: TaskId,
        /// The entry, relative to the task's directory.
        path: String,
        /// Whether it goes to the trash of the server's machine rather than being removed for good.
        trash: bool,
    }
);
op!(
    /// Find each line of a task's directory that contains a text.
    FileSearch = "file.search" -> Matches {
        /// The task's id.
        task: TaskId,
        /// The literal text to find.
        pattern: String,
    }
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shown {
    pub panel: BrowserId,
    pub rev: u64,
}

op!(
    /// Show a page in one of a task's browser panels on every client that renders the task, loading it again when the
    /// panel already shows it.
    BrowserOpen = "browser.open" -> Shown {
        /// The task's id.
        task: TaskId,
        /// The page: an http or https URL, one on localhost:<port> reaching that port on the server's machine, or file:// and an absolute path on the server's machine.
        url: String,
        /// `new` for a new browser panel, `background` for a new one no client selects or loads until it is chosen, or an existing browser panel's id.
        panel: BrowserTarget,
    }
);
op!(
    /// Record the page a client's browser panel navigated to, which no other client loads until the panel reopens.
    BrowserReport = "browser.report" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The browser panel's id.
        panel: BrowserId,
        /// The page the browser panel shows.
        url: String,
        /// The page's title, empty when it has none.
        title: String,
    }
);
op!(
    /// Close one of a task's browser panels on every client.
    BrowserClose = "browser.close" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The browser panel's id.
        panel: BrowserId,
    }
);
op!(
    /// Move one of a task's browser panels to another place among them, on every client.
    BrowserMove = "browser.move" -> Revised {
        /// The task's id.
        task: TaskId,
        /// The browser panel's id.
        panel: BrowserId,
        /// The index the browser panel takes among the task's browser panels, the first at 0.
        to: u16,
    }
);

/// An op as a CLI command: `<noun>.<verb>` runs as `octopus <noun> <verb>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub op: &'static str,
    pub doc: &'static str,
    pub fields: Vec<Field>,
}

fn command<O: Op>() -> Command {
    Command { op: O::NAME, doc: O::DOC.trim(), fields: O::fields() }
}

/// Every op a client runs on its own; the session ops and `pairing.pair` belong to the connection that carries them.
pub fn commands() -> Vec<Command> {
    vec![
        command::<WorkspaceAdd>(),
        command::<WorkspaceList>(),
        command::<WorkspaceRename>(),
        command::<WorkspaceSetColor>(),
        command::<WorkspaceRemove>(),
        command::<FolderGet>(),
        command::<HarnessSet>(),
        command::<HarnessList>(),
        command::<HarnessCheck>(),
        command::<TaskCreate>(),
        command::<TaskMove>(),
        command::<TaskSetParent>(),
        command::<TaskSetWorkspace>(),
        command::<TaskRename>(),
        command::<TaskSetSession>(),
        command::<AgentReportSession>(),
        command::<TaskArchive>(),
        command::<TaskFocus>(),
        command::<TaskGet>(),
        command::<TermOpen>(),
        command::<TermInstallMicrophone>(),
        command::<TermClose>(),
        command::<TermSend>(),
        command::<TermTakeLock>(),
        command::<TermResize>(),
        command::<FileList>(),
        command::<FileWrite>(),
        command::<FileCreate>(),
        command::<FileMove>(),
        command::<FileCopy>(),
        command::<FileDelete>(),
        command::<FileSearch>(),
        command::<BrowserOpen>(),
        command::<BrowserReport>(),
        command::<BrowserClose>(),
        command::<BrowserMove>(),
        command::<PairingOffer>(),
        command::<PairingList>(),
        command::<PairingForget>(),
        command::<ServerVersion>(),
    ]
}
