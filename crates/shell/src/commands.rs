use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use octopus_client::CallError;
use octopus_proto::Error;
use octopus_proto::account::{AccountId, Link};
use octopus_proto::device::PublicKey;
use octopus_proto::ops::{
    BrowserClose, BrowserMove, BrowserOpen, Entry, EntryKind, FileCopy, FileCreate, FileDelete, FileList, FileMove,
    FileSearch, FileWrite, Folder, FolderGet, HarnessCheck, HarnessStatus, Match, Offer, PairedAccount, PairingForget,
    PairingList, PairingOffer, TaskArchive, TaskCreate, TaskMove, TaskRename, TaskSetParent, TaskSetWorkspace,
    TermClaim, TermClose, TermInstallMicrophone, TermOpen, TermResize, TermTakeLock, WorkspaceAdd, WorkspaceRemove,
};
use octopus_proto::pairing::Invite;
use octopus_proto::settings::Setting;
use octopus_proto::types::{
    BrowserId, BrowserTarget, Color, Mode, PtyId, SessionStart, Swatch, TaskId, TaskState, WorkspaceId,
};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{Runtime, State};
use ts_rs::TS;

use crate::hub::{AccountView, Hub, Model, ServerView};
use crate::lock;
use crate::served_files::ServedFiles;
use crate::task_lists::{TaskRef, auto_swatch, awaits_resume};
use crate::terminal::Terminals;
use crate::ui::{Ui, UiAct, UiStore};

type Hubbed<'a> = State<'a, Arc<Hub>>;
type Uis<'a> = State<'a, Arc<UiStore>>;

pub(crate) struct Launched(pub Instant);

/// An offer as the window shows it, beside its invite drawn as an SVG QR code.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub(crate) struct ShownOffer {
    pub offer: Offer,
    pub qr: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub(crate) struct Boot {
    /// This client's name, which the server on its local socket knows it by.
    pub me: String,
    /// This client's device key, which every server it is paired with knows it by.
    #[ts(type = "string")]
    pub key: PublicKey,
    pub account: AccountView,
    pub servers: Vec<ServerView>,
    pub model: Model,
    pub ui: Ui,
    /// The version of the binary this window runs in.
    pub version: String,
}

#[tauri::command]
pub fn boot(hub: Hubbed, ui: Uis) -> Boot {
    let (me, key, account) = (hub.me.name(), hub.me.key.public(), hub.account());
    Boot {
        me,
        key,
        account,
        servers: hub.servers(),
        model: hub.model(),
        ui: ui.ui(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

#[tauri::command]
pub fn ready<R: Runtime>(webview: tauri::Webview<R>, launched: State<Launched>) {
    eprintln!("octopus app: {} interactive {} ms after launch", webview.label(), launched.0.elapsed().as_millis());
}

#[tauri::command]
pub fn ui_act(ui: Uis, act: UiAct) -> Result<(), String> {
    ui.act(act)
}

/// Pairs with the server `invite` names, lists it, and resolves to its name. Re-pairing a listed server, to pin a new
/// key, is the CLI's `pairing accept`; this window keeps the pins it started with.
#[tauri::command]
pub async fn server_pair(hub: Hubbed<'_>, invite: Invite) -> Result<String, String> {
    if let Some(name) = hub.listed_as(&invite.server) {
        return Err(format!("{} is listed as {name}; re-pair it with `octopus pairing accept`", invite.server));
    }
    hub.pair(&invite).await.map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn server_offer(hub: Hubbed<'_>, server: String) -> Result<ShownOffer, String> {
    let offer = hub.call(&server, &PairingOffer {}).await?;
    let qr = octopus_client::qr::svg(&offer.invite).map_err(|error| format!("{error:#}"))?;
    Ok(ShownOffer { qr, offer })
}

#[tauri::command]
pub async fn server_accounts(hub: Hubbed<'_>, server: String) -> Result<Vec<PairedAccount>, String> {
    Ok(hub.call(&server, &PairingList {}).await?.accounts)
}

#[tauri::command]
pub async fn server_revoke(hub: Hubbed<'_>, server: String, account: AccountId) -> Result<(), String> {
    hub.call(&server, &PairingForget { account }).await.map(drop)
}

/// A link as the window shows it, beside the link drawn as an SVG QR code.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub(crate) struct ShownLink {
    pub link: String,
    /// When the link stops joining, in milliseconds since 1970.
    pub expires_at: i64,
    pub qr: String,
}

#[tauri::command]
pub async fn account_link(hub: Hubbed<'_>) -> Result<ShownLink, String> {
    let (link, expires_at) = hub.link().await.map_err(|error| format!("{error:#}"))?;
    let qr = octopus_client::qr::svg(&link).map_err(|error| format!("{error:#}"))?;
    Ok(ShownLink { qr, link: link.to_string(), expires_at })
}

/// Joins the account the link names, naming the part of the link that is wrong.
#[tauri::command]
pub async fn account_join(hub: Hubbed<'_>, link: String) -> Result<(), String> {
    let link: Link = link.trim().parse()?;
    octopus_client::servers::join(&link, &hub.me).await.map_err(|error| format!("{error:#}"))
}

/// Sets the setting on every device of the account, resolving to the account as it then reads.
#[tauri::command]
pub fn settings_set(hub: Hubbed, setting: Setting) -> Result<AccountView, String> {
    hub.me
        .account
        .update_settings(|settings| settings.set(&hub.me.key, setting))
        .map_err(|error| format!("{error:#}"))?;
    Ok(hub.account())
}

#[tauri::command]
pub fn account_remove(hub: Hubbed, device: String) -> Result<(), String> {
    hub.remove_device(&device).map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub fn account_leave(hub: Hubbed) -> Result<(), String> {
    hub.start_afresh(true).map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub fn account_start_over(hub: Hubbed) -> Result<(), String> {
    hub.start_afresh(false).map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub async fn servers_refresh(hub: Hubbed<'_>) -> Result<(), String> {
    hub.refresh().await
}

#[tauri::command]
pub fn server_rename(hub: Hubbed, server: String, name: String) -> Result<(), String> {
    hub.rename_server(&server, &name).map_err(|error| format!("{error:#}"))
}

#[tauri::command]
pub fn server_forget(hub: Hubbed, server: String) -> Result<(), String> {
    hub.forget_server(&server).map_err(|error| format!("{error:#}"))
}

fn picked(swatch: Option<Swatch>) -> Color {
    swatch.map_or(Color::Auto, Color::Picked)
}

#[tauri::command]
pub fn auto_color(name: String) -> Swatch {
    auto_swatch(&name)
}

#[tauri::command]
pub async fn workspace_add(
    hub: Hubbed<'_>,
    server: String,
    name: String,
    root: String,
    swatch: Option<Swatch>,
) -> Result<(), String> {
    hub.call(&server, &WorkspaceAdd { name, root, color: picked(swatch) }).await.map(drop)
}

#[tauri::command]
pub async fn workspace_remove(hub: Hubbed<'_>, server: String, workspace: WorkspaceId) -> Result<(), String> {
    hub.call(&server, &WorkspaceRemove { workspace }).await.map(drop)
}

#[tauri::command]
pub async fn section_rename(hub: Hubbed<'_>, section: String, name: String) -> Result<(), String> {
    hub.rename_section(&section, &name).await
}

#[tauri::command]
pub async fn section_color(hub: Hubbed<'_>, section: String, swatch: Option<Swatch>) -> Result<(), String> {
    hub.color_section(&section, picked(swatch)).await
}

#[tauri::command]
pub async fn task_rename(hub: Hubbed<'_>, at: TaskRef, title: String) -> Result<(), String> {
    hub.call(&at.server, &TaskRename { task: at.task, title }).await.map(drop)
}

#[tauri::command]
pub async fn task_archive(hub: Hubbed<'_>, at: TaskRef) -> Result<(), String> {
    hub.call(&at.server, &TaskArchive { task: at.task }).await.map(drop)
}

/// Focuses the task the client shows, leaving an agent that awaits Resume ended.
#[tauri::command]
pub async fn task_focus(hub: Hubbed<'_>, at: TaskRef) -> Result<(), String> {
    if awaits_resume(&hub.task(&at)?) {
        return Ok(());
    }
    hub.focus(at).await
}

#[tauri::command]
pub async fn task_resume(hub: Hubbed<'_>, at: TaskRef) -> Result<(), String> {
    hub.focus(at).await
}

#[tauri::command]
pub async fn task_move(hub: Hubbed<'_>, at: TaskRef, state: TaskState) -> Result<(), String> {
    hub.call(&at.server, &TaskMove { task: at.task, state }).await.map(drop)
}

/// A task nests only under a task of its own server.
#[tauri::command]
pub async fn task_set_parent(hub: Hubbed<'_>, at: TaskRef, parent: Option<TaskRef>) -> Result<(), String> {
    let parent = match parent {
        Some(parent) if parent.server != at.server => {
            return Err(format!("{} is on {}, and {} on {}", parent.task, parent.server, at.task, at.server));
        }
        parent => parent.map(|parent| parent.task),
    };
    hub.call(&at.server, &TaskSetParent { task: at.task, parent }).await.map(drop)
}

/// Files the task with its subtasks in `workspace`, a workspace of the task's own server.
#[tauri::command]
pub async fn task_set_workspace(hub: Hubbed<'_>, at: TaskRef, workspace: WorkspaceId) -> Result<(), String> {
    hub.call(&at.server, &TaskSetWorkspace { task: at.task, workspace }).await.map(drop)
}

/// Every field of `task.create`, plus the server that creates it.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewTask {
    pub server: String,
    pub workspace: WorkspaceId,
    pub folder: String,
    pub mode: Mode,
    pub title: String,
    pub parent: Option<TaskId>,
    pub state: TaskState,
    pub harness: String,
    /// `FRESH_SESSION` for a fresh session, or an existing session's id for the agent to resume.
    #[ts(type = "string")]
    pub session: SessionStart,
}

/// Resolves to the created task's key.
#[tauri::command]
pub async fn task_create(hub: Hubbed<'_>, task: NewTask) -> Result<String, String> {
    let NewTask { server, workspace, folder, mode, title, parent, state, harness, session } = task;
    let create = TaskCreate { workspace, folder, mode, title, parent, state, harness, session };
    let created = hub.call(&server, &create).await?;
    Ok(TaskRef { server, task: created.task }.key())
}

#[tauri::command]
pub async fn folder_get(hub: Hubbed<'_>, server: String, folder: String) -> Result<Folder, String> {
    hub.call(&server, &FolderGet { folder }).await
}

/// None when the server has no such harness.
#[tauri::command]
pub async fn harness_check(hub: Hubbed<'_>, server: String, harness: String) -> Result<Option<HarnessStatus>, String> {
    match hub.connected(&server).await?.call(&HarnessCheck { id: harness }).await {
        Ok(status) => Ok(Some(status)),
        Err(CallError::Server(Error::NotFound { .. })) => Ok(None),
        Err(error) => Err(hub.failure(&server, error)),
    }
}

#[tauri::command]
pub async fn term_attach(
    hub: Hubbed<'_>,
    terminals: State<'_, Terminals>,
    at: TaskRef,
    pty: PtyId,
    output: Channel<InvokeResponseBody>,
) -> Result<u64, String> {
    let connection = hub.connected(&at.server).await?;
    terminals.attach(&connection, pty, output).await.map_err(|error| hub.failure(&at.server, error))
}

#[tauri::command]
pub fn term_input(terminals: State<Terminals>, attachment: u64, keys: String) -> Result<(), String> {
    terminals.input(attachment, keys.into_bytes())
}

#[tauri::command]
pub fn term_detach(terminals: State<Terminals>, attachment: u64) {
    terminals.detach(attachment);
}

/// Resolves to the id of the shell panel it opens.
#[tauri::command]
pub async fn term_open(hub: Hubbed<'_>, at: TaskRef, path: String) -> Result<PtyId, String> {
    Ok(hub.call(&at.server, &TermOpen { task: at.task, path }).await?.panel)
}

/// Resolves to the id of the shell panel it opens, which runs the server's microphone install.
#[tauri::command]
pub async fn term_install_microphone(hub: Hubbed<'_>, at: TaskRef) -> Result<PtyId, String> {
    Ok(hub.call(&at.server, &TermInstallMicrophone { task: at.task }).await?.panel)
}

#[tauri::command]
pub async fn term_close(hub: Hubbed<'_>, at: TaskRef, panel: PtyId) -> Result<(), String> {
    hub.call(&at.server, &TermClose { task: at.task, panel: panel.0 }).await.map(drop)
}

/// A claim that loses the race to another client is no failure: that client holds the terminal, and this one draws
/// it cropped.
#[tauri::command]
pub async fn term_claim(hub: Hubbed<'_>, at: TaskRef, panel: PtyId, cols: u16, rows: u16) -> Result<(), String> {
    let connection = hub.connected(&at.server).await?;
    match connection.call(&TermClaim { task: at.task, panel: panel.0, cols, rows }).await {
        Ok(_) | Err(CallError::Server(Error::PtyLocked { .. })) => Ok(()),
        Err(error) => Err(hub.failure(&at.server, error)),
    }
}

#[tauri::command]
pub async fn term_take_lock(hub: Hubbed<'_>, at: TaskRef, panel: PtyId, cols: u16, rows: u16) -> Result<(), String> {
    hub.call(&at.server, &TermTakeLock { task: at.task, panel: panel.0, cols, rows }).await.map(drop)
}

#[tauri::command]
pub async fn term_resize(hub: Hubbed<'_>, at: TaskRef, panel: PtyId, cols: u16, rows: u16) -> Result<(), String> {
    hub.call(&at.server, &TermResize { task: at.task, panel: panel.0, cols, rows }).await.map(drop)
}

/// Shows `url` in a browser panel of the task on its server, which pushes the panel to every client; resolves to
/// the panel's id.
#[tauri::command]
pub async fn browser_open(
    hub: Hubbed<'_>,
    at: TaskRef,
    panel: BrowserTarget,
    url: String,
) -> Result<BrowserId, String> {
    Ok(hub.call(&at.server, &BrowserOpen { task: at.task, url, panel }).await?.panel)
}

/// Removes a browser panel of the task on its server, which every client then stops rendering.
#[tauri::command]
pub async fn browser_close(hub: Hubbed<'_>, at: TaskRef, panel: BrowserId) -> Result<(), String> {
    hub.call(&at.server, &BrowserClose { task: at.task, panel }).await.map(drop)
}

/// Moves a browser panel of the task on its server to index `to` among the task's panels, on every client.
#[tauri::command]
pub async fn browser_move(hub: Hubbed<'_>, at: TaskRef, panel: BrowserId, to: u16) -> Result<(), String> {
    hub.call(&at.server, &BrowserMove { task: at.task, panel, to }).await.map(drop)
}

#[tauri::command]
pub async fn file_list(hub: Hubbed<'_>, at: TaskRef, path: String) -> Result<Vec<Entry>, String> {
    Ok(hub.call(&at.server, &FileList { task: at.task, path }).await?.entries)
}

#[tauri::command]
pub fn file_root_url(served: State<'_, Arc<ServedFiles>>, at: TaskRef) -> String {
    served.task(at)
}

#[tauri::command]
pub async fn file_write(hub: Hubbed<'_>, at: TaskRef, path: String, content: String) -> Result<(), String> {
    hub.call(&at.server, &FileWrite { task: at.task, path, content }).await.map(drop)
}

#[tauri::command]
pub async fn file_create(hub: Hubbed<'_>, at: TaskRef, path: String, kind: EntryKind) -> Result<(), String> {
    hub.call(&at.server, &FileCreate { task: at.task, path, kind }).await.map(drop)
}

#[tauri::command]
pub async fn file_move(hub: Hubbed<'_>, at: TaskRef, from: String, to: String) -> Result<(), String> {
    hub.call(&at.server, &FileMove { task: at.task, from, to }).await.map(drop)
}

#[tauri::command]
pub async fn file_copy(hub: Hubbed<'_>, at: TaskRef, from: String, to: String) -> Result<(), String> {
    hub.call(&at.server, &FileCopy { task: at.task, from, to }).await.map(drop)
}

#[tauri::command]
pub async fn file_delete(hub: Hubbed<'_>, at: TaskRef, path: String, trash: bool) -> Result<(), String> {
    hub.call(&at.server, &FileDelete { task: at.task, path, trash }).await.map(drop)
}

/// The files and folders the user handed the window by drop or paste, the only ones on this machine a client copies to
/// a server.
#[derive(Default)]
pub struct Offered(Mutex<HashSet<PathBuf>>);

impl Offered {
    pub fn add(&self, paths: &[PathBuf]) {
        lock(&self.0).extend(paths.iter().cloned());
    }
}

/// Copies `local`, a dropped or pasted file or folder, to the new `path` of the task's directory, a folder with every
/// file and folder in it.
#[tauri::command]
pub async fn file_upload(
    hub: Hubbed<'_>,
    offered: State<'_, Offered>,
    at: TaskRef,
    path: String,
    local: PathBuf,
) -> Result<(), String> {
    if !lock(&offered.0).contains(&local) {
        return Err(format!("{} was neither dropped nor pasted", local.display()));
    }
    let connection = hub.connected(&at.server).await?;
    let mut pending = vec![(path, local, true)];
    while let Some((path, local, top)) = pending.pop() {
        let read = |error: std::io::Error| format!("{}: {error}", local.display());
        let entry = if top { tokio::fs::metadata(&local).await } else { tokio::fs::symlink_metadata(&local).await };
        let entry = entry.map_err(read)?;
        if entry.is_dir() {
            hub.call(&at.server, &FileCreate { task: at.task.clone(), path: path.clone(), kind: EntryKind::Directory })
                .await?;
            let mut inside = tokio::fs::read_dir(&local).await.map_err(read)?;
            while let Some(child) = inside.next_entry().await.map_err(read)? {
                let name = child.file_name().to_string_lossy().into_owned();
                pending.push((format!("{path}/{name}"), child.path(), false));
            }
        } else if entry.is_file() {
            let content = tokio::fs::File::open(&local).await.map_err(read)?;
            let upload = connection.upload(at.task.clone(), path, entry.len(), content).await;
            upload.map_err(|error| hub.failure(&at.server, error))?;
        } else {
            return Err(format!("{} is neither a file nor a folder", local.display()));
        }
    }
    Ok(())
}

/// Sends `content`, the base64 of a file this client holds rather than one on its disk, as the new `path` of the task's
/// directory.
#[tauri::command]
pub async fn file_upload_content(hub: Hubbed<'_>, at: TaskRef, path: String, content: String) -> Result<(), String> {
    use base64::Engine;
    let content = base64::engine::general_purpose::STANDARD
        .decode(content)
        .map_err(|error| format!("decode the uploaded content: {error}"))?;
    let connection = hub.connected(&at.server).await?;
    let upload = connection.upload(at.task, path, content.len() as u64, content.as_slice()).await;
    upload.map_err(|error| hub.failure(&at.server, error))
}

#[tauri::command]
pub async fn file_search(hub: Hubbed<'_>, at: TaskRef, pattern: String) -> Result<Vec<Match>, String> {
    Ok(hub.call(&at.server, &FileSearch { task: at.task, pattern }).await?.matches)
}

/// Closes the app; its servers run on as their own services.
#[tauri::command]
pub fn quit<R: Runtime>(app: tauri::AppHandle<R>) {
    app.exit(0);
}
