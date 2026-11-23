#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::Duration;

use octopus_client::account::Book;
use octopus_client::servers::Address;
use octopus_client::{CallError, Connection, Identity};
use octopus_proto::device::DeviceKey;
use octopus_proto::local::Listener;
use octopus_proto::ops::{HarnessSet, TaskCreate, TaskFocus, TaskGet, TermSend, WorkspaceAdd, WorkspaceList};
use octopus_proto::remote::{self, RelayUrl};
use octopus_proto::types::{Color, Mode, Panel, ReadDir, SessionStart, Stream, Task, TaskId, TaskState, WorkspaceId};
use octopus_server::{Server, TerminalHost, serve};
use tempfile::TempDir;
use tokio::io::{AsyncRead, AsyncReadExt};

pub const SERVER: &str = "mac-mini";
pub const OCTOPUS: &str = env!("CARGO_BIN_EXE_octopus");

/// A relay URL that refuses every connection, for a server whose tests reach it over the local socket alone.
const NO_RELAY: &str = "http://127.0.0.1:9/";

/// A server in this process, which ends its terminal host and every agent on drop.
pub struct Running {
    pub server: Arc<Server>,
    pub socket: PathBuf,
    host: PathBuf,
    relay: Option<iroh_relay::server::Server>,
}

impl Drop for Running {
    fn drop(&mut self) {
        end_host(&self.host);
    }
}

/// Serves `dir`'s store on `socket`, keeping terminals in the host at `host`. A fresh `host` on the same `dir` plays a
/// machine restart: the server finds none of its terminals.
pub async fn start(dir: &Path, socket: &str, host: &str) -> Running {
    serve_on(dir, socket, host, None).await
}

/// A server started in a fresh directory, which lives as long as the returned `TempDir`.
pub async fn server() -> (TempDir, Running) {
    let dir = tempfile::tempdir().expect("tempdir");
    let running = start(dir.path(), "server.sock", "host.sock").await;
    (dir, running)
}

/// As `server`, homed on a relay of its own, served on loopback over plain HTTP for as long as the server runs.
pub async fn relayed_server() -> (TempDir, Running) {
    let mut config = iroh_relay::server::ServerConfig::default();
    config.relay = Some(iroh_relay::server::RelayConfig::new(([127, 0, 0, 1], 0)));
    let relay = iroh_relay::server::Server::spawn(config).await.expect("spawn a relay");
    let dir = tempfile::tempdir().expect("tempdir");
    let running = serve_on(dir.path(), "server.sock", "host.sock", Some(relay)).await;
    (dir, running)
}

async fn serve_on(dir: &Path, socket: &str, host: &str, relay: Option<iroh_relay::server::Server>) -> Running {
    let url = relay
        .as_ref()
        .map_or(NO_RELAY.into(), |relay| format!("http://{}", relay.http_addr().expect("the relay serves HTTP")));
    let url: RelayUrl = url.parse().expect("a URL");
    let socket = dir.join(socket);
    let host = TerminalHost { program: OCTOPUS.into(), address: dir.join(host) };
    octopus_server::lay_out_files(&dir.join("harnesses"), "the skill", Path::new(OCTOPUS)).expect("lay out files");
    let server = Server::open(
        SERVER,
        &dir.join("store.db"),
        &dir.join("worktrees"),
        &host,
        url.clone(),
        false,
        &dir.join("harnesses"),
    )
    .expect("open server");
    let endpoint = remote::listen(([127, 0, 0, 1], 0).into(), url, server.device()).await.expect("bind iroh");
    tokio::spawn(serve(Arc::clone(&server), Listener::bind(&socket).expect("bind"), endpoint));
    Running { server, socket, host: host.address, relay }
}

/// Kills every process the host's terminals started, then the host, as a machine shutdown would. Each terminal's
/// program leads a session of its own, so killing its group never reaches the test runner.
fn end_host(address: &Path) {
    let host = format!("terminal-host --address {}", address.display());
    let pids = |args: &[&str]| {
        let found = Command::new("pgrep").args(args).output().map(|output| output.stdout).unwrap_or_default();
        String::from_utf8_lossy(&found).split_whitespace().map(str::to_owned).collect::<Vec<_>>()
    };
    let pkill = |args: &[&str]| Command::new("pkill").arg("-KILL").args(args).status();
    for program in pids(&["-f", &host]).iter().flat_map(|host| pids(&["-P", host])) {
        let _detached_children_gone = pkill(&["-P", &program]);
        let _group_gone = pkill(&["-g", &program]);
    }
    let _gone = pkill(&["-f", &host]);
}

/// A client named `name` with `key`, which another identity of the same key makes a second process of one device.
pub fn identity(name: &str, key: &DeviceKey) -> Identity {
    let key = DeviceKey::from_pkcs8(key.pkcs8()).expect("a key");
    let account = Book::memory(&key);
    Identity::new(name.into(), key, account).expect("an identity")
}

pub fn local(socket: &Path) -> Address {
    Address::Local { socket: socket.to_path_buf() }
}

/// A client named `name`, with a key of its own, on the server's local socket.
pub async fn connect(socket: &Path, name: &str) -> Connection {
    Connection::connect(&local(socket), &identity(name, &DeviceKey::generate())).await.expect("connect")
}

/// An `octopus server run` process, killed with its terminal host and every agent when dropped.
pub struct ServerProcess {
    child: Child,
    pub socket: PathBuf,
    host: Option<PathBuf>,
}

fn server_run(dir: &Path) -> Command {
    let mut command = Command::new(OCTOPUS);
    command
        .args(["server", "run", "--name", SERVER, "--socket"])
        .arg(dir.join("server.sock"))
        .arg("--store")
        .arg(dir.join("store.db"))
        .arg("--worktrees")
        .arg(dir.join("worktrees"))
        .arg("--terminal-host")
        .arg(dir.join("host.sock"))
        .args(["--udp", "127.0.0.1:0", "--relay", NO_RELAY, "--microphone", "false"])
        .env("HOME", dir.join("home"));
    command
}

impl ServerProcess {
    pub async fn run(dir: &Path) -> Self {
        Self::serve(dir, server_run(dir)).await
    }

    /// Started the way launchd starts a service: a bare PATH, and `home` as the account's home.
    pub async fn launched(dir: &Path, home: &Path) -> Self {
        let mut command = server_run(dir);
        command.env_clear().env("HOME", home).env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        Self::serve(dir, command).await
    }

    async fn serve(dir: &Path, mut command: Command) -> Self {
        let child = command.spawn().expect("spawn octopus server run");
        let process = Self { child, socket: dir.join("server.sock"), host: Some(dir.join("host.sock")) };
        let probe = identity("probe", &DeviceKey::generate());
        eventually("the server answering", async || Connection::connect(&local(&process.socket), &probe).await.is_ok())
            .await;
        process
    }

    /// Ends the process the way a crash does, with no chance to clean up, leaving its terminals to the next server.
    pub fn kill(mut self) {
        self.host = None;
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _already_dead = self.child.kill();
        let _reaped = self.child.wait();
        if let Some(host) = &self.host {
            end_host(host);
        }
    }
}

/// An agent that prints `<line>`, then stays open reading input, which the terminal echoes.
pub fn echo_agent(line: &str) -> Vec<String> {
    vec!["sh".into(), "-c".into(), format!("echo {line}; exec cat")]
}

/// An agent that probes its terminal for status reports, reports `status` once its terminal answers, then acts as
/// `echo_agent`.
pub fn reporting_agent(status: &str, line: &str) -> Vec<String> {
    let probe = r"'\033]7501;?\033\\'";
    let script = format!(
        "stty raw -echo; printf {probe}; answer=$(dd bs=1 count=10 2>/dev/null); stty sane; \
         [ \"$answer\" = \"$(printf {probe})\" ] && printf '\\033]7501;state={status}:app=test\\033\\\\'; \
         echo {line}; exec cat"
    );
    vec!["sh".into(), "-c".into(), script]
}

/// What `cat` in a terminal prints back for a program reporting `status`, as the text of a `term.send`.
pub fn status_report(status: &str) -> String {
    format!("\u{1b}]7501;state={status}:app=test\u{1b}\\")
}

/// A harness that is always installed and names no session.
pub fn harness(id: &str, argv: Vec<String>) -> HarnessSet {
    HarnessSet { id: id.into(), argv, install: "true".into(), resume: vec![], env: vec![] }
}

pub async fn set_harness(connection: &Connection, id: &str, argv: Vec<String>) {
    connection.call(&harness(id, argv)).await.expect("harness.set");
}

pub async fn focus(connection: &Connection, id: &TaskId) {
    connection.call(&TaskFocus { task: id.clone(), args: vec![] }).await.expect("task.focus");
}

/// The stream of the task's agent terminal.
pub async fn agent(connection: &Connection, id: &TaskId) -> Stream {
    Stream::Pty(task(connection, id).await.panels[0].id.clone())
}

/// Types `text` and enter into `panel`, taking its lock.
pub fn send(id: &TaskId, panel: &str, text: &str) -> TermSend {
    TermSend { task: id.clone(), panel: panel.into(), text: text.into(), enter: true, take_lock: true }
}

pub fn workspace_add(root: &Path) -> WorkspaceAdd {
    WorkspaceAdd { name: "octopus".into(), root: root.to_string_lossy().to_string(), color: Color::Auto }
}

pub async fn workspace(connection: &Connection, root: &Path) -> WorkspaceId {
    connection.call(&workspace_add(root)).await.expect("workspace.add").workspace
}

/// Creates the pinned root task "Strict serde" in `folder`, on a fresh session.
pub fn task_create(workspace: &WorkspaceId, folder: &Path, mode: Mode, harness: &str) -> TaskCreate {
    TaskCreate {
        workspace: workspace.clone(),
        folder: folder.to_string_lossy().into_owned(),
        mode,
        title: "Strict serde".into(),
        parent: None,
        state: TaskState::Pinned,
        harness: harness.into(),
        session: SessionStart::Fresh,
    }
}

/// Creates the pinned root task "Strict serde" in the workspace's root, on a fresh session.
pub async fn create(connection: &Connection, workspace: &WorkspaceId, mode: Mode, harness: &str) -> TaskId {
    let listed = connection.call(&WorkspaceList {}).await.expect("workspace.list").workspaces;
    let root = listed.into_iter().find(|listed| &listed.id == workspace).expect("the workspace").root;
    connection.call(&task_create(workspace, Path::new(&root), mode, harness)).await.expect("task.create").task
}

pub async fn task(connection: &Connection, task: &TaskId) -> Task {
    connection.call(&TaskGet { task: task.clone() }).await.expect("task.get")
}

pub fn refused<T: std::fmt::Debug>(outcome: Result<T, CallError>) -> octopus_proto::Error {
    match outcome {
        Err(CallError::Server(error)) => error,
        other => panic!("expected the server to refuse, got {other:?}"),
    }
}

/// The label of the panel's lock holder, the way every client shows it.
pub fn holder_name(panel: &Panel) -> Option<&str> {
    panel.lock_holder.as_ref().map(|holder| holder.name.as_str())
}

/// A read's file size and the bytes it sent.
pub async fn read(
    connection: &Connection,
    dir: ReadDir,
    path: &str,
    start: u64,
    len: u64,
) -> Result<(u64, Vec<u8>), CallError> {
    let (size, mut opened) = connection.read(dir, path.into(), start, len).await?;
    let mut bytes = Vec::new();
    opened.read_to_end(&mut bytes).await.expect("read the file");
    Ok((size, bytes))
}

/// Everything the stream sent until `needle` arrived.
pub async fn read_until(stream: &mut (impl AsyncRead + Unpin), needle: &str) -> String {
    let mut seen = Vec::new();
    let mut buffer = [0u8; 4096];
    let found = tokio::time::timeout(Duration::from_secs(5), async {
        while !String::from_utf8_lossy(&seen).contains(needle) {
            let read = stream.read(&mut buffer).await.expect("read stream");
            assert!(read > 0, "stream ended before {needle:?}; saw {:?}", String::from_utf8_lossy(&seen));
            seen.extend_from_slice(&buffer[..read]);
        }
    })
    .await;
    assert!(found.is_ok(), "no {needle:?} within 5 s; saw {:?}", String::from_utf8_lossy(&seen));
    String::from_utf8_lossy(&seen).into_owned()
}

pub async fn eventually(what: &str, mut check: impl AsyncFnMut() -> bool) {
    let reached = tokio::time::timeout(Duration::from_secs(8), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "{what} did not happen within 8 s");
}
