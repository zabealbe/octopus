// Rendered from the Rust types by `cargo run -p octopus-desktop --example types`. Edit those, not this.

export type TaskId = string;

export type WorkspaceId = string;

export type PtyId = string;

export type BrowserId = string;

export type BrowserTarget = BrowserId | typeof NEW_BROWSER_PANEL | typeof BACKGROUND_BROWSER_PANEL;

export type SessionId = string;

export type TaskState = "pinned" | "temporary" | "completed" | "cancelled";

export type Mode = "in_place" | "worktree" | "copy";

export type AgentStatus = "working" | "waiting" | "idle";

export type Hue = "red" | "orange" | "amber" | "lime" | "green" | "mint" | "teal" | "sky" | "blue" | "indigo" | "violet" | "pink";

export type Tone = "mist" | "soft" | "vivid" | "deep";

export type Swatch = { hue: Hue, tone: Tone, };

export type Agent = { "kind": "not_started" } | { "kind": "running", status: AgentStatus, } | { "kind": "harness_not_installed", install: string, } | { "kind": "harness_unknown" };

export type Panel = { id: PtyId, name: string, running: boolean, lock_holder: LockHolder | null, cols: number, rows: number, listening: Listening, };

export type Listening = "no" | "yes" | "no_microphone";

export type LockHolder = { name: string, key: string | null, };

export type BrowserPanel = { panel: BrowserId, 
/**
 * The page the panel last showed on any client.
 */
url: string, 
/**
 * That page's title, empty until a client reports one.
 */
title: string, 
/**
 * How many times `browser.open` has shown a page in the panel; each client loads `url` whenever it moves.
 */
load: number, };

export type Task = { id: TaskId, workspace: WorkspaceId, 
/**
 * The directory on the server's machine the task works in, fixed for its life.
 */
folder: string, mode: Mode, 
/**
 * Where the task runs: the folder, or the same folder inside the task's worktree.
 */
directory: string, 
/**
 * The worktree's branch, or null for an `in_place` task.
 */
branch: string | null, parent: TaskId | null, title: string, state: TaskState, harness: string, 
/**
 * The conversation the task's agent runs in, which its next start resumes, or null until it has one.
 */
session: SessionId | null, ended_at: number | null, archived_at: number | null, rev: number, agent: Agent, panels: Array<Panel>, browsers: Array<BrowserPanel>, };

export type EntryKind = "file" | "directory";

export type Entry = { name: string, kind: EntryKind, };

export type Match = { path: string, line: number, text: string, };

export type Subfolder = { name: string, 
/**
 * Whether the server's OS guards it, so reading it can wait on the OS asking its user.
 */
guarded: boolean, };

export type OsSetting = { 
/**
 * Where the setting sits, as the OS names it, such as `System Settings › Privacy & Security › Files & Folders`.
 */
name: string, 
/**
 * The URL that opens it.
 */
url: string, };

export type Inside = { "kind": "listed", folders: Array<Subfolder>, } | { "kind": "kept_out", setting: OsSetting | null, };

export type Folder = { 
/**
 * The folder, its symlinks, `.` and `..` resolved.
 */
folder: string, 
/**
 * The top of the git repo the folder sits in, or null outside one.
 */
repo: string | null, 
/**
 * The branch the repo's HEAD is on, which a worktree branches from, or null outside a repo or on a detached HEAD.
 */
branch: string | null, 
/**
 * The folders inside it, or what lets the server in when its OS keeps the server out.
 */
inside: Inside, };

export type HarnessStatus = { 
/**
 * The program the harness's argv starts.
 */
program: string, 
/**
 * Whether the PATH of the user's login environment holds the program.
 */
installed: boolean, 
/**
 * The command that installs the program.
 */
install: string, };

export type Harnesses = { 
/**
 * Every harness id the server knows, sorted.
 */
harnesses: Array<string>, };

export type Offer = { invite: string, 
/**
 * When the invite stops pairing, in milliseconds since 1970.
 */
expires_at: number, };

export type ShownOffer = { offer: Offer, qr: string, };

export type PairedClient = { 
/**
 * The name the device last gave, else the one its account gives it.
 */
name: string, 
/**
 * The device's key, which is who it is.
 */
key: string, 
/**
 * Whether this server runs on the device's machine, which reaches it over its local socket.
 */
hosts: boolean, connected: boolean, 
/**
 * When one of its connections last opened or closed, in milliseconds since 1970; null if it never connected.
 */
seen_at: number | null, };

export type PairedAccount = { account: string, devices: Array<PairedClient>, };

export type ShownLink = { link: string, 
/**
 * When the link stops joining, in milliseconds since 1970.
 */
expires_at: number, qr: string, };

export type AccountView = { 
/**
 * The first eight hex digits of the account's id.
 */
id: string, 
/**
 * Whether the account still holds this client.
 */
member: boolean, devices: Array<DeviceView>, settings: Current, };

export type DeviceView = { name: string, 
/**
 * The device's key, in hex.
 */
key: string, 
/**
 * Whether it is this client.
 */
this: boolean, };

export type TaskRef = { server: string, task: TaskId, };

export type Glyph = "working" | "waiting" | "idle" | "asleep" | "ended" | "waking" | "completed" | "cancelled";

export type Row = { key: string, at: TaskRef, title: string, depth: number, 
/**
 * The task's own state, which a subtask's group or column may differ from.
 */
state: TaskState, glyph: Glyph, };

export type Column = { state: TaskState, rows: Array<Row>, };

export type Section = { name: string, 
/**
 * The section's color, with auto resolved.
 */
swatch: Swatch, 
/**
 * The color auto resolves to for the section's name.
 */
auto: Swatch, live: number, 
/**
 * Whether the agent of one of its live tasks is waiting on the user.
 */
waiting: boolean, 
/**
 * The sidebar's groups, one per live state, of the live tasks.
 */
groups: Array<Column>, 
/**
 * The board's columns, one per state in `TaskState::ALL` order, of the unarchived tasks.
 */
columns: Array<Column>, };

export type WorkspaceRef = { server: string, workspace: WorkspaceId, name: string, 
/**
 * The directory on the server's machine.
 */
root: string, };

export type Lists = { sections: Array<Section>, workspaces: Array<WorkspaceRef>, };

export type PanelView = { 
/**
 * Whether this client holds the panel's lock.
 */
held_here: boolean, id: PtyId, name: string, running: boolean, lock_holder: LockHolder | null, cols: number, rows: number, listening: Listening, };

export type TaskView = { key: string, at: TaskRef, section: string, 
/**
 * The workspace, then each ancestor root first, then the task.
 */
crumbs: Array<string>, live: boolean, glyph: Glyph, 
/**
 * The task with its panels taken out: they reach the UI through `panels` alone.
 */
task: Omit<Task, "panels">, panels: Array<PanelView>, };

export type TermScreen = { cols: number, rows: number, };

export type ServerView = { name: string, 
/**
 * What the window calls the server: "local" for one on the local socket, else its name.
 */
label: string, 
/**
 * The relay a remote server is dialled through; none for a server on the local socket.
 */
relay: string | null, 
/**
 * The key a remote server is pinned by, in hex; none for a server on the local socket.
 */
key: string | null, reachable: boolean, 
/**
 * When `reachable` last changed, in milliseconds since 1970.
 */
since: number, 
/**
 * The round trip of the connection's newest heartbeat.
 */
rtt_ms: number | null, problem: string | null, };

export type Model = { 
/**
 * Raised by every model built, so the UI drops one that arrives after a newer one.
 */
version: number, lists: Lists, tasks: { [key in string]: TaskView }, 
/**
 * The servers this client lists that have not answered since the app started, whose workspaces and tasks the
 * lists cannot hold yet.
 */
unread: Array<string>, };

export type FilesChanged = { key: string, paths: Array<string>, };

export type View = "task" | "tasks";

export type Surface = "files" | "browser";

export type Split = number;

export type Modal = "palette" | "new_task" | "pair_server" | "join_account" | "shortcuts";

export type Appearance = "system" | "light" | "dark";

export type Zoom = number;

export type ZoomStep = "in" | "out" | "reset";

export type AgentFlags = string;

export type Setting = { "setting": "section_order", "value": Array<string> } | { "setting": "collapsed", "value": Array<string> } | { "setting": "agent_flags", "value": { [key in string]: AgentFlags } };

export type Current = { section_order: Array<string>, collapsed: Array<string>, agent_flags: { [key in string]: AgentFlags }, };

export type Prefs = { pinned: boolean, surface: Surface, split: Split, view: View, 
/**
 * The open task's key.
 */
open: string | null, 
/**
 * The section the Tasks board shows.
 */
board: string | null, appearance: Appearance, zoom: Zoom, 
/**
 * Whether the app keeps the octopus skill installed for the user's Claude Code, which the first open asks.
 */
skill: boolean, };

export type Reveal = { 
/**
 * The task's key.
 */
task: string, path: string, 
/**
 * Counts reveals, so revealing the same file again opens it again.
 */
nth: number, };

export type Ui = { 
/**
 * Raised by every change, so a layer drops a state that arrives after a newer one.
 */
version: number, prefs: Prefs, 
/**
 * The opened tasks' keys, newest first.
 */
recent: Array<string>, sidebar: boolean, client_menu: boolean, modal: Modal | null, reveal: Reveal | null, problem: string | null, 
/**
 * The window's size in the UI's CSS pixels, which the zoom scales.
 */
viewport: Viewport, 
/**
 * Where the float layer takes the pointer over the layers beneath it.
 */
float_input: Array<Bounds>, };

export type UiAct = { "do": "open_task", key: string, } | { "do": "close_server", server: string, } | { "do": "show_view", view: View, } | { "do": "show_board", section: string, } | { "do": "show_surface", surface: Surface, } | { "do": "set_split", split: Split, } | { "do": "reset_split" } | { "do": "toggle_pin" } | { "do": "show_sidebar", shown: boolean, } | { "do": "toggle_sidebar" } | { "do": "show_client_menu", shown: boolean, } | { "do": "toggle_client_menu" } | { "do": "set_appearance", appearance: Appearance, } | { "do": "set_skill", skill: boolean, } | { "do": "zoom", step: ZoomStep, } | { "do": "show_modal", modal: Modal | null, } | { "do": "float", surface: string, at: Floated, modal: boolean, } | { "do": "sink", surface: string, } | { "do": "reveal", task: string, path: string, } | { "do": "report", problem: string, };

export type Viewport = { width: number, height: number, };

export type Boot = { 
/**
 * This client's name, which the server on its local socket knows it by.
 */
me: string, 
/**
 * This client's device key, which every server it is paired with knows it by.
 */
key: string, account: AccountView, servers: Array<ServerView>, model: Model, ui: Ui, 
/**
 * The version of the binary this window runs in.
 */
version: string, };

export type NewTask = { server: string, workspace: WorkspaceId, folder: string, mode: Mode, title: string, parent: TaskId | null, state: TaskState, harness: string, 
/**
 * `FRESH_SESSION` for a fresh session, or an existing session's id for the agent to resume.
 */
session: string, };

export type Bounds = { x: number, y: number, width: number, height: number, };

export type Floated = { input: Bounds, drawn: Bounds, body: Bounds, radius: number, };

export type FileDrag = { "kind": "over", x: number, y: number, } | { "kind": "drop", paths: Array<string>, x: number, y: number, } | { "kind": "leave" };

export type OnScreen = { at: TaskRef, panel: BrowserId, bounds: Bounds, };

export type Go = "back" | "forward" | "reload" | "reload_from_origin";

export type Paged = { 
/**
 * The task's key.
 */
task: string, panel: BrowserId, url: string, loading: boolean, back: boolean, forward: boolean, 
/**
 * The port of the server's machine the page reaches through a tunnel, when it does.
 */
tunnel: number | null, };

export type Failed = { task: string, panel: BrowserId, url: string, error: string, };

export type Opened = { task: string, panel: BrowserId, url: string, select: boolean, };

export type Held = { task: string, panel: BrowserId, link: string, };

export type Downloaded = { file: string, folder: string, success: boolean, };

export const TASK_STATES = ["pinned","temporary","completed","cancelled"] as const;

export const LIVE_STATES = ["pinned","temporary"] as const;

export const MODES = ["in_place","worktree","copy"] as const;

export const HUES = ["red","orange","amber","lime","green","mint","teal","sky","blue","indigo","violet","pink"] as const;

export const TONES = ["mist","soft","vivid","deep"] as const;

export const ZOOMS = [80,90,100,110,125,150] as const;

export const AGENT_PANEL = "agent" as const;

export const NEW_BROWSER_PANEL = "new" as const;

export const BACKGROUND_BROWSER_PANEL = "background" as const;

export const FRESH_SESSION = "fresh" as const;

export const TERM_HISTORY_LINES = 10000 as const;

export const HOLDER_GRACE_MS = 10000 as const;

export const COMMANDS = ["boot","ready","ui_act","quit","server_pair","server_offer","server_accounts","server_revoke","account_link","account_join","settings_set","account_remove","account_leave","account_start_over","servers_refresh","server_rename","server_forget","workspace_add","workspace_remove","section_rename","section_color","auto_color","task_focus","task_resume","task_move","task_set_parent","task_set_workspace","task_rename","task_archive","task_create","folder_get","harness_check","harness_list","term_attach","term_input","term_detach","term_open","term_install_microphone","term_close","term_claim","term_take_lock","term_resize","browser_open","browser_close","browser_move","file_list","file_root_url","file_write","file_create","file_move","file_copy","file_delete","file_upload","file_upload_content","file_search"] as const;

export const EVENTS = {"account":"account","browserDownloaded":"browser_downloaded","browserFailed":"browser_failed","browserHeld":"browser_held","browserOpened":"browser_opened","browserPaged":"browser_paged","fileDrag":"file-drag","filesChanged":"files_changed","microphoneDenied":"microphone_denied","model":"model","servers":"servers","ui":"ui","windowFocused":"window-focused","windowShown":"window-shown"} as const;
