//! The demo computer: a stand-in for a paired server that lives inside this client, for trying octopus with no server
//! of one's own. Entering `CODE` where an invite goes lists it in the account as pairing lists a server, and every
//! connection to its key and relay then opens in-memory streams this module answers with the protocol a server
//! speaks, so nothing else can tell it from one. It runs nothing and reaches no network: its agents replay recorded
//! sessions, its shells answer a few commands from a script, and its files are an invented project held in memory.
//!
//! There are two demo computers, both named tidepool. The one `CODE` lists holds a workspace of tasks at work. The
//! blank one, which the desktop's first open lists while its show tours the window, holds the project alone, and the
//! agent of each task made on it takes the request the show types and works on it.

mod project;
mod sessions;
mod terminal;
mod web;
mod world;

use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock};

use anyhow::bail;
use octopus_proto::account::Change;
use octopus_proto::device::PublicKey;
use octopus_proto::ops::{
    AccountSync, AgentReportSession, BrowserClose, BrowserMove, BrowserOpen, BrowserReport, Done, FileCopy, FileCreate,
    FileDelete, FileList, FileMove, FileSearch, FileWrite, FolderGet, HarnessCheck, HarnessList, HarnessSet,
    HarnessStatus, Harnesses, Offer, PairingForget, PairingList, PairingOffer, ServerVersion, SessionHello,
    SessionPing, SessionSync, Synced, TaskArchive, TaskCreate, TaskFocus, TaskGet, TaskMove, TaskRename, TaskSetParent,
    TaskSetSession, TaskSetWorkspace, TermClaim, TermClose, TermOpen, TermResize, TermSend, TermTakeLock, Version,
    Welcome, WorkspaceAdd, WorkspaceList, WorkspaceRemove, WorkspaceRename, WorkspaceSetColor,
};
use octopus_proto::remote::{self, RelayUrl};
use octopus_proto::types::{PtyId, ReadDir, Stream};
use octopus_proto::{
    Error, FileSize, Op, Outcome, Request, Response, StreamHeader, line, parse_args, parse_request, request_from,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, DuplexStream};
use tokio::sync::broadcast::error::RecvError;

use crate::connection::{DataStream, Identity};
use terminal::Typed;
use world::{Caller, Replay, World};

/// What a person types, pastes or scans where an invite goes to open the demo; no invite is spelled this way.
pub const CODE: &str = "octopus://demo";
pub const NAME: &str = "tidepool";
/// The project folder both demo computers hold.
pub const PROJECT: &str = "tidewatch";
/// The request the agent of a task made on the blank computer takes, which the desktop's show types as the task's title.
pub const FIRST_TASK: &str = "Add a weekly tide chart";

/// A demo computer: the one `CODE` pairs, or the blank one the first open's show tours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Computer {
    Tidepool,
    Blank,
}

impl Computer {
    const ALL: [Self; 2] = [Self::Tidepool, Self::Blank];

    /// The session a task's agent replays when it wakes on a fresh session, on the computer whose agents take a
    /// request.
    fn waking(self) -> Option<&'static [sessions::Beat]> {
        (self == Self::Blank).then_some(sessions::FIRST)
    }

    fn key(self) -> PublicKey {
        static TIDEPOOL: LazyLock<PublicKey> = LazyLock::new(|| {
            "ed25519:89d59ce09b75a51895943d11c5b478187ddd3c1b777797cd94cb12786f8a3634"
                .parse()
                .expect("a point on the curve")
        });
        static BLANK: LazyLock<PublicKey> = LazyLock::new(|| {
            "ed25519:a8a1598d1effa517da42e2d8a84f4f4eb6f2643b4b9f439701593d9a253b3bd2"
                .parse()
                .expect("a point on the curve")
        });
        match self {
            Self::Tidepool => *TIDEPOOL,
            Self::Blank => *BLANK,
        }
    }
}

/// A relay under `.invalid`, a name no resolver answers, so nothing ever dials it.
static RELAY: LazyLock<RelayUrl> = LazyLock::new(|| remote::relay("https://demo.invalid./").expect("an https URL"));

const STREAM_BUFFER: usize = 64 * 1024;
const READ_CHUNK: usize = 4096;

pub fn is_code(text: &str) -> bool {
    text.trim().eq_ignore_ascii_case(CODE)
}

/// The key the pairing sheet shows for `text`, when it is the demo code.
pub fn key_of(text: &str) -> Option<PublicKey> {
    is_code(text).then(|| Computer::Tidepool.key())
}

/// Lists `computer` in `me`'s account, as pairing lists a server, and resolves to its name.
pub fn pair(me: &Identity, computer: Computer) -> anyhow::Result<String> {
    let key = computer.key();
    if let Some(listed) = me.account.state().servers.into_iter().find(|listed| listed.key == key) {
        bail!("Already paired as {}.", listed.name);
    }
    me.account
        .append(&me.key, Change::ListServer { server: key, relay: RELAY.clone(), name: NAME.into(), host: None })?;
    Ok(NAME.into())
}

/// Takes `computer` out of `me`'s account, when it lists it, as forgetting a server does.
pub fn forget(me: &Identity, computer: Computer) -> anyhow::Result<()> {
    let key = computer.key();
    if me.account.state().servers.iter().any(|listed| listed.key == key) {
        me.account.append(&me.key, Change::ForgetServer { server: key })?;
    }
    Ok(())
}

/// The demo computer at `key` and `relay`, if one is.
pub(crate) fn reaches(key: &PublicKey, relay: &RelayUrl) -> Option<Computer> {
    let computer = Computer::ALL.into_iter().find(|computer| computer.key() == *key)?;
    (relay == &*RELAY).then_some(computer)
}

/// One client's way into a demo computer, which knows the client by the device key it dials from, as a server knows
/// a client by the key its handshake proves.
pub(crate) struct Line {
    demo: Arc<Demo>,
    client: PublicKey,
}

impl Line {
    pub(crate) fn dial(computer: Computer, client: PublicKey) -> Self {
        Self { demo: Demo::running(computer), client }
    }

    /// A fresh stream to the demo computer, which reads its first line as a request or a data stream's header.
    pub(crate) fn open(&self) -> DataStream {
        let (near, far) = tokio::io::duplex(STREAM_BUFFER);
        tokio::spawn(serve(Arc::clone(&self.demo), self.client, far));
        Box::new(near)
    }
}

pub(crate) struct Demo {
    world: Mutex<World>,
}

impl Demo {
    /// The demo computer `computer` of this process, started on its first dial and kept until the process ends.
    fn running(computer: Computer) -> Arc<Self> {
        static RUNNING: [OnceLock<Arc<Demo>>; Computer::ALL.len()] = [const { OnceLock::new() }; Computer::ALL.len()];
        let demo = RUNNING[computer as usize].get_or_init(|| {
            let (world, replays) = World::new(computer);
            let demo = Arc::new(Self { world: Mutex::new(world) });
            for replay in replays {
                demo.play(replay);
            }
            demo
        });
        Arc::clone(demo)
    }

    fn play(self: &Arc<Self>, replay: Replay) {
        match replay {
            Replay::Once(pty, beats) => drop(tokio::spawn(sessions::replay(Arc::clone(self), pty, beats))),
            Replay::Cycle(pty, recordings) => drop(tokio::spawn(sessions::cycle(Arc::clone(self), pty, recordings))),
        }
    }

    fn world(&self) -> MutexGuard<'_, World> {
        octopus_proto::lock(&self.world)
    }

    fn run_typed(self: &Arc<Self>, world: &mut World, pty: &PtyId, typed: Vec<Typed>) {
        for typed in typed {
            match typed {
                Typed::UnaskedRequest => drop(tokio::spawn(sessions::reply(Arc::clone(self), pty.clone()))),
                Typed::Exit => {
                    world.close(pty);
                }
            }
        }
    }

    fn answer(self: &Arc<Self>, caller: &mut Caller, request: &Request) -> Outcome {
        let mut world = self.world();
        let world = &mut *world;
        macro_rules! ops {
            ($($op:ty => |$args:pat_param| $body:expr;)*) => {
                $(if request.op == <$op as Op>::NAME {
                    return Outcome::from(parse_args::<$op>(&request.args).and_then(|$args| $body));
                })*
            };
        }
        let refused = |what: &str| Err::<Done, Error>(Error::conflict(format!("the demo computer {what}")));
        ops! {
            SessionHello => |op| {
                caller.name = op.client;
                Ok(Welcome { server: NAME.into(), rev: world.rev() })
            };
            SessionSync => |_| Ok(world.snapshot());
            SessionPing => |_| Ok(Done {});
            ServerVersion => |_| Ok(Version { version: env!("CARGO_PKG_VERSION").into() });
            AccountSync => |op| {
                world.keep_account(&op.account);
                Ok(Synced { account: op.account, settings: op.settings, server: world.hosted() })
            };
            PairingOffer => |_| Err::<Offer, Error>(Error::conflict("the demo pairs nothing"));
            PairingList => |_| Ok(world.accounts(caller));
            PairingForget => |_| refused("keeps every account");
            WorkspaceAdd => |op| world.workspace_add(op);
            WorkspaceList => |_| Ok(world.workspace_list());
            WorkspaceRename => |op| world.workspace_rename(op);
            WorkspaceSetColor => |op| world.workspace_set_color(op);
            WorkspaceRemove => |op| world.workspace_remove(op);
            FolderGet => |op| world.folder_get(op);
            HarnessSet => |op| Ok(world.harness_set(op.id));
            HarnessList => |_| Ok(Harnesses { harnesses: vec!["claude-code".into()] });
            HarnessCheck => |_| Ok(HarnessStatus { program: "claude".into(), installed: true, install: String::new() });
            TaskCreate => |op| world.task_create(op);
            TaskSetSession => |op| world.task_set_session(op);
            AgentReportSession => |op| world.task_set_session(TaskSetSession { task: op.task, session: op.session });
            TaskMove => |op| world.task_move(op);
            TaskSetParent => |op| world.task_set_parent(op);
            TaskSetWorkspace => |op| world.task_set_workspace(op);
            TaskRename => |op| world.task_rename(op);
            TaskArchive => |op| world.task_archive(op);
            TaskFocus => |op| world.task_focus(op).map(|(revised, replay)| {
                if let Some(replay) = replay {
                    self.play(replay);
                }
                revised
            });
            TaskGet => |op| world.task_get(op);
            TermOpen => |op| world.term_open(op);
            TermClose => |op| world.term_close(op);
            TermSend => |op| world.term_send(caller, op).map(|(revised, pty, typed)| {
                self.run_typed(world, &pty, typed);
                revised
            });
            TermClaim => |op| world.term_claim(caller, op);
            TermTakeLock => |op| world.term_take_lock(caller, op);
            TermResize => |op| world.term_resize(caller, op);
            FileList => |op| world.file_list(op);
            FileWrite => |op| world.file_write(op);
            FileCreate => |op| world.file_create(op);
            FileMove => |op| world.file_move(op);
            FileCopy => |op| world.file_copy(op);
            FileDelete => |op| world.file_delete(op);
            FileSearch => |op| world.file_search(op);
            BrowserOpen => |op| world.browser_open(op);
            BrowserReport => |op| world.browser_report(op);
            BrowserClose => |op| world.browser_close(op);
            BrowserMove => |op| world.browser_move(op);
        }
        Outcome::Err(Error::UnknownOp { op: request.op.clone() })
    }
}

async fn send_line<W: AsyncWrite + Unpin>(write: &mut W, frame: &impl serde::Serialize) -> std::io::Result<()> {
    write.write_all(line(frame).as_bytes()).await
}

async fn serve(demo: Arc<Demo>, client: PublicKey, stream: DuplexStream) {
    let (read, mut write) = tokio::io::split(stream);
    let mut read = BufReader::new(read);
    let mut first = String::new();
    if !matches!(read.read_line(&mut first).await, Ok(1..)) {
        return;
    }
    let first: Value = match serde_json::from_str(&first) {
        Ok(first) => first,
        Err(error) => {
            let refused = Response { id: 0, outcome: Outcome::Err(Error::invalid(error)) };
            return drop(send_line(&mut write, &refused).await);
        }
    };
    if first.get("open").is_none() {
        return control(demo, Caller { key: client, name: String::new() }, request_from(first), read, write).await;
    }
    match serde_json::from_value::<StreamHeader>(first) {
        Ok(header) => data(demo, Caller { key: client, name: header.client }, header.open, read, write).await,
        Err(error) => drop(send_line(&mut write, &Outcome::Err(Error::invalid(error))).await),
    }
}

fn opened() -> Outcome {
    Outcome::Ok(Value::Null)
}

/// The control stream: answers each request in turn, and sends every change the demo pushes between them.
async fn control(
    demo: Arc<Demo>,
    mut caller: Caller,
    first: Result<Request, (u64, Error)>,
    read: impl AsyncRead + Unpin,
    mut write: impl AsyncWrite + Unpin,
) {
    let mut pushes = demo.world().pushes();
    let mut lines = BufReader::new(read).lines();
    let mut next = Some(first);
    loop {
        let request = match next.take() {
            Some(request) => request,
            None => tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(line)) => parse_request(&line),
                    _ => return,
                },
                push = pushes.recv() => match push {
                    Ok(push) => {
                        if send_line(&mut write, &push).await.is_err() {
                            return;
                        }
                        continue;
                    }
                    Err(RecvError::Lagged(_) | RecvError::Closed) => return,
                },
            },
        };
        let response = match request {
            Ok(request) => Response { id: request.id, outcome: demo.answer(&mut caller, &request) },
            Err((id, error)) => Response { id, outcome: Outcome::Err(error) },
        };
        if send_line(&mut write, &response).await.is_err() {
            return;
        }
    }
}

/// A data stream: the open's outcome line, then the bytes of the resource its header names.
async fn data<R, W>(demo: Arc<Demo>, caller: Caller, open: Stream, read: R, mut write: W)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    match open {
        Stream::Pty(pty) => terminal(demo, caller, pty, read, write).await,
        Stream::Read { dir: ReadDir::Task(task), path, start, len } => {
            let file = demo.world().file(&task, &path).map(<[u8]>::to_vec);
            let file = match file {
                Ok(file) => file,
                Err(error) => {
                    let _client_gone = send_line(&mut write, &Outcome::Err(error)).await;
                    return;
                }
            };
            let size = file.len() as u64;
            let from = start.min(size);
            let to = from.saturating_add(len).min(size);
            let reply = Outcome::from(Ok::<_, Error>(FileSize { size }));
            if send_line(&mut write, &reply).await.is_ok()
                && write.write_all(&file[from as usize..to as usize]).await.is_ok()
            {
                let _client_gone = write.shutdown().await;
            }
        }
        Stream::Read { dir: ReadDir::Page(dir), .. } => {
            let _client_gone =
                send_line(&mut write, &Outcome::Err(Error::not_found(format!("{dir} on the demo computer")))).await;
        }
        Stream::Upload { task, bytes, path } => {
            let allowed = demo.world().may_upload(&task, bytes);
            if let Err(error) = allowed {
                return drop(send_line(&mut write, &Outcome::Err(error)).await);
            }
            if send_line(&mut write, &opened()).await.is_err() {
                return;
            }
            let mut content = Vec::new();
            let landed = match read.take(bytes.saturating_add(1)).read_to_end(&mut content).await {
                Ok(_) if content.len() as u64 == bytes => demo.world().upload(&task, &path, content),
                Ok(read) => Err(Error::invalid(format!("the upload carried {read} bytes of {bytes}"))),
                Err(error) => Err(Error::internal(error)),
            };
            let _client_gone = send_line(&mut write, &Outcome::from(landed)).await;
        }
        Stream::Tunnel(web::PORT) => {
            if send_line(&mut write, &opened()).await.is_ok() {
                web::serve(tokio::io::join(read, write)).await;
            }
        }
        Stream::Tunnel(port) => {
            let nothing = Error::internal(format!("nothing listens on port {port} of the demo computer"));
            let _client_gone = send_line(&mut write, &Outcome::Err(nothing)).await;
        }
        Stream::Mic(_) => {
            let deaf = Error::conflict("the demo computer's terminals hear no microphone");
            let _client_gone = send_line(&mut write, &Outcome::Err(deaf)).await;
        }
    }
}

/// Shows the terminal from its screen on, and types what the client sends into it while the client holds its lock.
async fn terminal(
    demo: Arc<Demo>,
    caller: Caller,
    pty: PtyId,
    mut read: impl AsyncRead + Unpin,
    mut write: impl AsyncWrite + Unpin,
) {
    let attached = demo.world().attach(&pty, caller.key);
    let (screen, mut pieces) = match attached {
        Ok(attached) => attached,
        Err(error) => {
            let _client_gone = send_line(&mut write, &Outcome::Err(error)).await;
            return;
        }
    };
    let mut showing = send_line(&mut write, &opened()).await.is_ok() && screen.write_to(&mut write).await.is_ok();
    let mut keys = [0; READ_CHUNK];
    while showing {
        showing = tokio::select! {
            piece = pieces.recv() => match piece {
                Ok(piece) => piece.write_to(&mut write).await.is_ok(),
                Err(_) => false,
            },
            typed = read.read(&mut keys) => match typed {
                Ok(count @ 1..) => {
                    let text = String::from_utf8_lossy(&keys[..count]).into_owned();
                    let mut world = demo.world();
                    let typed = world.type_into(&pty, &caller.key, &text);
                    demo.run_typed(&mut world, &pty, typed);
                    true
                }
                _ => false,
            },
        };
    }
    demo.world().detach(&pty, caller.key);
    let _client_gone = write.shutdown().await;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use octopus_proto::device::DeviceKey;
    use octopus_proto::ops::{
        FolderGet, Inside, SessionSync, TaskCreate, TaskFocus, TaskGet, TermClaim, TermOpen, WorkspaceAdd,
    };
    use octopus_proto::term::Piece;
    use octopus_proto::types::{AGENT_PANEL, Color, Mode, PtyId, SessionStart, Stream, TaskId, TaskState};
    use tokio::io::{AsyncWriteExt, BufReader};

    use super::Computer;
    use crate::account::Book;
    use crate::servers::Address;
    use crate::{Connection, Identity};

    /// Takes the terminal's lock, types `keys`, and waits for `shown` to appear on its screen.
    async fn type_and_see(connection: &Connection, task: TaskId, pty: PtyId, keys: &[u8], shown: &str) {
        let mut stream = connection.open_data_stream(&Stream::Pty(pty.clone())).await.expect("attach");
        connection.call(&TermClaim { task, panel: pty.0, cols: 80, rows: 24 }).await.expect("the free lock");
        stream.write_all(keys).await.expect("type");
        let mut screen = Vec::new();
        let mut pieces = BufReader::new(stream);
        while !String::from_utf8_lossy(&screen).contains(shown) {
            let piece = tokio::time::timeout(Duration::from_secs(3), Piece::read_from(&mut pieces)).await;
            match piece.expect("output in time").expect("a piece").expect("the stream goes on") {
                Piece::Screen(drawn) => screen.extend_from_slice(&drawn.bytes),
                Piece::Output(bytes) => screen.extend_from_slice(&bytes),
            }
        }
    }

    #[tokio::test]
    async fn the_demo_code_lists_a_server_whose_shell_and_agent_answer_their_holder() {
        let key = DeviceKey::generate();
        let book = Book::memory(&key);
        let me = Identity::new("phone".into(), key, book).expect("an identity");
        assert!(super::is_code(" OCTOPUS://demo\n"));
        assert_eq!(super::pair(&me, Computer::Tidepool).expect("listed"), super::NAME);
        assert!(super::pair(&me, Computer::Tidepool).is_err(), "a second pairing pairs nothing");
        let listed = me.account.state().servers.into_iter().next().expect("the demo is listed");
        let connection =
            Connection::connect(&Address::Remote { key: listed.key, relay: listed.relay }, &me).await.expect("connect");
        let snapshot = connection.call(&SessionSync { since_rev: None }).await.expect("sync");
        let unstarted = snapshot.tasks.iter().find(|task| task.state.is_live() && task.panels.is_empty());
        let task = unstarted.expect("a task whose agent never started").id.clone();
        let shell = connection.call(&TermOpen { task: task.clone(), path: ".".into() }).await.expect("a shell");
        type_and_see(&connection, task.clone(), shell.panel, b"echo ahoy\r", "\r\nahoy\r\n").await;
        connection.call(&TaskFocus { task: task.clone(), args: Vec::new() }).await.expect("focus");
        let focused = connection.call(&TaskGet { task: task.clone() }).await.expect("the task");
        let agent = focused.panels.into_iter().find(|panel| panel.name == AGENT_PANEL).expect("a fresh agent").id;
        type_and_see(&connection, task, agent, b"ship it\r", "the demo computer").await;
    }

    #[tokio::test]
    async fn the_blank_demo_starts_empty_and_its_first_task_takes_a_request() {
        let key = DeviceKey::generate();
        let book = Book::memory(&key);
        let me = Identity::new("laptop".into(), key, book).expect("an identity");
        super::pair(&me, Computer::Blank).expect("listed");
        let listed = me.account.state().servers.into_iter().next().expect("the blank demo is listed");
        let connection =
            Connection::connect(&Address::Remote { key: listed.key, relay: listed.relay }, &me).await.expect("connect");
        let snapshot = connection.call(&SessionSync { since_rev: None }).await.expect("sync");
        assert!(snapshot.workspaces.is_empty() && snapshot.tasks.is_empty(), "the blank demo holds nothing");
        let home = connection.call(&FolderGet { folder: "~".into() }).await.expect("home");
        let Inside::Listed { folders } = home.inside else { panic!("the blank demo lists its home") };
        assert_eq!(folders.iter().map(|folder| folder.name.as_str()).collect::<Vec<_>>(), [super::PROJECT]);
        let color = Color::parse("auto").expect("auto");
        let added = WorkspaceAdd { name: "tidewatch".into(), root: "/work/tidewatch".into(), color };
        let workspace = connection.call(&added).await.expect("a workspace").workspace;
        let create = TaskCreate {
            workspace,
            folder: "/work/tidewatch".into(),
            mode: Mode::InPlace,
            title: super::FIRST_TASK.into(),
            parent: None,
            state: TaskState::Temporary,
            harness: "claude-code".into(),
            session: SessionStart::Fresh,
        };
        let task = connection.call(&create).await.expect("a task").task;
        connection.call(&TaskFocus { task: task.clone(), args: Vec::new() }).await.expect("focus");
        let focused = connection.call(&TaskGet { task: task.clone() }).await.expect("the task");
        let agent = focused.panels.into_iter().find(|panel| panel.name == AGENT_PANEL).expect("a fresh agent").id;
        type_and_see(&connection, task, agent, b"", "Writing src/chart.js").await;
        super::forget(&me, Computer::Blank).expect("forgotten");
        assert!(me.account.state().servers.is_empty());
    }
}
