use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::device::PublicKey;
use crate::schema::{Arg, Shape, arg_shape};

macro_rules! id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            pub fn random() -> Self {
                Self(format!("{}{:08x}", $prefix, fastrand::u32(..)))
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id!(WorkspaceId, "wsp_");
id!(TaskId, "tsk_");
id!(PtyId, "pty_");
id!(BrowserId, "brw_");

arg_shape!(Shape::Workspace => WorkspaceId);
arg_shape!(Shape::Text => TaskId, BrowserId, BrowserTarget, SessionId);
arg_shape!(choice(&Self::ALL) => TaskState, Mode);

/// The browser panel `browser.open` shows a page in: `new`, `background`, or an existing panel's id.
#[derive(Debug, Clone, PartialEq, Eq, TS)]
#[ts(type = "BrowserId | typeof NEW_BROWSER_PANEL | typeof BACKGROUND_BROWSER_PANEL")]
pub enum BrowserTarget {
    New,
    /// A new browser panel that no client selects or loads until it is chosen.
    Background,
    Panel(BrowserId),
}

/// How `browser.open` names a browser panel it is to add.
pub const NEW_BROWSER_PANEL: &str = "new";
/// How `browser.open` names a browser panel it is to add behind the selected one.
pub const BACKGROUND_BROWSER_PANEL: &str = "background";

impl Serialize for BrowserTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::New => serializer.serialize_str(NEW_BROWSER_PANEL),
            Self::Background => serializer.serialize_str(BACKGROUND_BROWSER_PANEL),
            Self::Panel(id) => id.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for BrowserTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(match text.as_str() {
            NEW_BROWSER_PANEL => Self::New,
            BACKGROUND_BROWSER_PANEL => Self::Background,
            _ => Self::Panel(BrowserId(text)),
        })
    }
}

impl Arg for Option<TaskId> {
    fn shape() -> Shape {
        Shape::Nullable { value: Box::new(Shape::Text), none: "a root task" }
    }
}

/// A conversation of a task's agent, as its harness names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(transparent)]
pub struct SessionId(pub String);

/// The session `task.create` gives a task: `fresh`, or an existing session's id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStart {
    Fresh,
    Resume(SessionId),
}

/// How `task.create` asks for a fresh session.
pub const FRESH_SESSION: &str = "fresh";

impl Serialize for SessionStart {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Fresh => serializer.serialize_str(FRESH_SESSION),
            Self::Resume(id) => id.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for SessionStart {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(if text == FRESH_SESSION { Self::Fresh } else { Self::Resume(SessionId(text)) })
    }
}

impl Arg for SessionStart {
    fn shape() -> Shape {
        Shape::Keyword { word: FRESH_SESSION, means: "a fresh session" }
    }
}

impl Arg for Option<u64> {
    fn shape() -> Shape {
        Shape::Nullable { value: Box::new(Shape::Integer), none: "none" }
    }
}

pub(crate) fn choice<T: Serialize>(all: &[T]) -> Shape {
    Shape::Choice(all.iter().map(to_wire).collect())
}

pub const ENV_TASK: &str = "OCTOPUS_TASK";
pub const ENV_SERVER: &str = "OCTOPUS_SERVER";
pub const AGENT_PANEL: &str = "agent";

/// Parses a unit enum from its wire spelling, e.g. `"pinned"`.
pub fn from_wire<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(text.to_string())).map_err(|error| error.to_string())
}

/// The wire spelling of a unit enum, e.g. `"pinned"`.
pub fn to_wire<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => text,
        other => panic!("{other:?} is not a unit enum's wire spelling"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pinned,
    Temporary,
    Completed,
    Cancelled,
}

impl TaskState {
    pub const ALL: [Self; 4] = [Self::Pinned, Self::Temporary, Self::Completed, Self::Cancelled];
    pub const LIVE: [Self; 2] = [Self::Pinned, Self::Temporary];

    pub fn is_live(self) -> bool {
        Self::LIVE.contains(&self)
    }
}

/// Where a task runs: in its folder, or in the same folder inside a worktree of its own on a branch of its own, which a
/// copy starts with the repo's ignored files cloned in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    InPlace,
    Worktree,
    Copy,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::InPlace, Self::Worktree, Self::Copy];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Hue {
    Red,
    Orange,
    Amber,
    Lime,
    Green,
    Mint,
    Teal,
    Sky,
    Blue,
    Indigo,
    Violet,
    Pink,
}

impl Hue {
    pub const ALL: [Self; 12] = [
        Self::Red,
        Self::Orange,
        Self::Amber,
        Self::Lime,
        Self::Green,
        Self::Mint,
        Self::Teal,
        Self::Sky,
        Self::Blue,
        Self::Indigo,
        Self::Violet,
        Self::Pink,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Mist,
    Soft,
    Vivid,
    Deep,
}

impl Tone {
    pub const ALL: [Self; 4] = [Self::Mist, Self::Soft, Self::Vivid, Self::Deep];
}

/// One cell of the color matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Swatch {
    pub hue: Hue,
    pub tone: Tone,
}

/// A workspace's color: `auto`, which each client resolves from the name, or one swatch, spelled `violet-vivid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Auto,
    Picked(Swatch),
}

impl Color {
    pub fn all() -> impl Iterator<Item = Self> {
        let swatches = Tone::ALL.into_iter().flat_map(|tone| Hue::ALL.into_iter().map(move |hue| Swatch { hue, tone }));
        std::iter::once(Self::Auto).chain(swatches.map(Self::Picked))
    }

    pub fn name(self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Picked(Swatch { hue, tone }) => format!("{}-{}", to_wire(&hue), to_wire(&tone)),
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        Self::all().find(|color| color.name() == text).ok_or_else(|| format!("{text:?} is not auto or a <hue>-<tone>"))
    }
}

impl Serialize for Color {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl Arg for Color {
    fn shape() -> Shape {
        Shape::Choice(Self::all().map(Self::name).collect())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub root: String,
    pub color: Color,
}

/// How long a holder whose connection dropped keeps its locks.
pub const HOLDER_GRACE_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Panel {
    pub id: PtyId,
    pub name: String,
    pub running: bool,
    pub lock_holder: Option<LockHolder>,
    pub cols: u16,
    pub rows: u16,
    pub listening: Listening,
}

/// Whether a program of the terminal records, and whether the holder's microphone can reach it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Listening {
    No,
    /// A program records from the terminal's virtual microphone, so the holder sends its own on `Stream::Mic`.
    Yes,
    /// A program records while a client on another machine holds the terminal, and the server has no working virtual
    /// microphone to carry the holder's; `term.install_microphone` installs one.
    NoMicrophone,
}

/// Who holds a terminal's lock: a paired device by its key, labelled with the name it is pinned under, or a client on
/// the server's local socket by its name alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LockHolder {
    pub name: String,
    #[ts(type = "string | null")]
    pub key: Option<PublicKey>,
}

impl Panel {
    pub fn answers_to(&self, key: &str) -> bool {
        self.name == key || self.id.0 == key
    }
}

/// A page a browser panel may show: a web page, or a file at an absolute path on the server's machine.
pub fn web_page(url: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(url).map_err(|error| format!("{url}: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        "file" if parsed.host().is_none() => Ok(parsed),
        "file" => Err(format!("{url}: a file page names a path on the server's machine, with no host")),
        scheme => Err(format!("a browser panel shows http, https and file pages, not {scheme}:")),
    }
}

/// A browser panel of a task, which every client of the task renders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BrowserPanel {
    pub panel: BrowserId,
    /// The page the panel last showed on any client.
    pub url: String,
    /// That page's title, empty until a client reports one.
    pub title: String,
    /// How many times `browser.open` has shown a page in the panel; each client loads `url` whenever it moves.
    pub load: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Working,
    Waiting,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Agent {
    NotStarted,
    Running { status: AgentStatus },
    HarnessNotInstalled { install: String },
    HarnessUnknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: TaskId,
    pub workspace: WorkspaceId,
    /// The directory on the server's machine the task works in, fixed for its life.
    pub folder: String,
    pub mode: Mode,
    /// Where the task runs: the folder, or the same folder inside the task's worktree.
    pub directory: String,
    /// The worktree's branch, or null for an `in_place` task.
    pub branch: Option<String>,
    pub parent: Option<TaskId>,
    pub title: String,
    pub state: TaskState,
    pub harness: String,
    /// The conversation the task's agent runs in, which its next start resumes, or null until it has one.
    pub session: Option<SessionId>,
    pub ended_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub rev: u64,
    pub agent: Agent,
    pub panels: Vec<Panel>,
    pub browsers: Vec<BrowserPanel>,
}

/// A data stream, which a client opens by naming it in its `StreamHeader`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Stream {
    Pty(PtyId),
    /// Up to `len` bytes of the file `path` names under `dir`, from byte `start`: fewer where the file ends first, and
    /// none from a `start` at or past its end. The open reply carries the file's `FileSize`.
    Read {
        dir: ReadDir,
        path: String,
        start: u64,
        len: u64,
    },
    /// A new file's `bytes`, which the client sends and finishes; the server then answers with one outcome line. The
    /// count tells a finished upload from one whose sender died.
    Upload {
        task: TaskId,
        bytes: u64,
        path: String,
    },
    /// A TCP connection to a port on the server's machine.
    Tunnel(u16),
    /// The lock holder's microphone, as `MIC_FORMAT`, into the terminal's virtual microphone.
    Mic(PtyId),
}

/// The directory a `Stream::Read` path is relative to and never leads out of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadDir {
    /// A task's directory.
    Task(TaskId),
    /// A file page's root: the directory, an absolute path on the server's machine, of the file the page loads from.
    Page(String),
}

impl Stream {
    /// The `len` that reads a file to its end.
    pub const TO_END: u64 = u64::MAX;
}

/// The audio on a `Stream::Mic`.
pub struct PcmFormat {
    pub rate: u32,
    pub channels: u16,
    pub bytes_per_sample: usize,
}

impl PcmFormat {
    pub const fn bytes_per_second(&self) -> usize {
        self.rate as usize * self.channels as usize * self.bytes_per_sample
    }
}

/// Signed 16-bit little-endian, 48 kHz, mono.
pub const MIC_FORMAT: PcmFormat = PcmFormat { rate: 48_000, channels: 1, bytes_per_sample: 2 };

/// The virtual microphone a macOS server's HAL driver adds: the name it shows and the UID it is found by.
pub struct MicDevice {
    pub name: &'static str,
    pub uid: &'static str,
}

pub const MIC_DEVICE: MicDevice = MicDevice { name: "Octopus Microphone", uid: "octopus-microphone" };
