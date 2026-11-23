import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { COMMANDS, EVENTS } from "./generated.ts";
import type {
  AccountView,
  Boot,
  BrowserId,
  BrowserTarget,
  Entry,
  EntryKind,
  FileDrag,
  FilesChanged,
  Folder,
  HarnessStatus,
  Harnesses,
  Match,
  Model,
  NewTask,
  ShownLink,
  ShownOffer,
  PairedAccount,
  PtyId,
  ServerView,
  Setting,
  Swatch,
  TaskRef,
  TaskState,
  TermScreen,
  Ui,
  UiAct,
  WorkspaceId,
} from "./generated.ts";

const octopus = (command: (typeof COMMANDS)[number]) => `plugin:octopus|${command}`;

/** The IPC carries bytes as a JSON array, a number each that both ends handle one by one; as base64 they travel as
 * one string a third longer than they are. */
const base64 = (content: Blob) =>
  new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = reader.result as string;
      resolve(url.slice(url.indexOf(",") + 1));
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(content);
  });

export const backend = {
  boot: () => invoke<Boot>(octopus("boot")),
  ready: () => invoke<void>(octopus("ready")),
  uiAct: (act: UiAct) => invoke<void>(octopus("ui_act"), { act }),
  quit: () => invoke<void>(octopus("quit")),
  /** Resolves to the paired server's name. */
  serverPair: (invite: string) => invoke<string>(octopus("server_pair"), { invite }),
  serverOffer: (server: string) => invoke<ShownOffer>(octopus("server_offer"), { server }),
  serverAccounts: (server: string) => invoke<PairedAccount[]>(octopus("server_accounts"), { server }),
  /** Stops the server trusting the account, which is its id in full. */
  serverRevoke: (server: string, account: string) => invoke<void>(octopus("server_revoke"), { server, account }),
  /** A fresh link to this account, held by a reachable server for 10 minutes. */
  accountLink: () => invoke<ShownLink>(octopus("account_link")),
  /** Joins the account the link names, bringing this client's servers along. */
  accountJoin: (link: string) => invoke<void>(octopus("account_join"), { link }),
  /** Removes the device whose key, in hex, is `device`. */
  accountRemove: (device: string) => invoke<void>(octopus("account_remove"), { device }),
  accountLeave: () => invoke<void>(octopus("account_leave")),
  /** Sets the setting on every device of the account, resolving to the account as it then reads. */
  settingsSet: (setting: Setting) => invoke<AccountView>(octopus("settings_set"), { setting }),
  accountStartOver: () => invoke<void>(octopus("account_start_over")),
  /** Resyncs every connected server, rejecting with each one's problem. */
  serversRefresh: () => invoke<void>(octopus("servers_refresh")),
  /** Renames the server on every device of the account. */
  serverRename: (server: string, name: string) => invoke<void>(octopus("server_rename"), { server, name }),
  /** Forgets the server on every device of the account. */
  serverForget: (server: string) => invoke<void>(octopus("server_forget"), { server }),
  /** A null swatch adds the workspace with the auto color. */
  workspaceAdd: (server: string, name: string, root: string, swatch: Swatch | null) =>
    invoke<void>(octopus("workspace_add"), { server, name, root, swatch }),
  autoColor: (name: string) => invoke<Swatch>(octopus("auto_color"), { name }),
  workspaceRemove: (server: string, workspace: WorkspaceId) =>
    invoke<void>(octopus("workspace_remove"), { server, workspace }),
  /** Renames the section's workspace on every server of the section. */
  sectionRename: (section: string, name: string) => invoke<void>(octopus("section_rename"), { section, name }),
  /** Colors the section's workspace on every server of the section; a null swatch sets auto. */
  sectionColor: (section: string, swatch: Swatch | null) => invoke<void>(octopus("section_color"), { section, swatch }),
  taskFocus: (at: TaskRef) => invoke<void>(octopus("task_focus"), { at }),
  taskResume: (at: TaskRef) => invoke<void>(octopus("task_resume"), { at }),
  taskMove: (at: TaskRef, state: TaskState) => invoke<void>(octopus("task_move"), { at, state }),
  taskSetParent: (at: TaskRef, parent: TaskRef | null) => invoke<void>(octopus("task_set_parent"), { at, parent }),
  /** Files the task with its subtasks in `workspace`, a workspace of the task's own server. */
  taskSetWorkspace: (at: TaskRef, workspace: WorkspaceId) =>
    invoke<void>(octopus("task_set_workspace"), { at, workspace }),
  /** The folder on `server`, resolved, with the git repo it sits in and the folders inside it. */
  folderGet: (server: string, folder: string) => invoke<Folder>(octopus("folder_get"), { server, folder }),
  /** Opens a system setting of this machine by its URL. */
  openSetting: (url: string) => openUrl(url),
  /** Whether the harness's program is installed on `server`; null when the server has no such harness. */
  harnessCheck: (server: string, harness: string) =>
    invoke<HarnessStatus | null>(octopus("harness_check"), { server, harness }),
  harnessList: (server: string) => invoke<Harnesses>(octopus("harness_list"), { server }),
  taskRename: (at: TaskRef, title: string) => invoke<void>(octopus("task_rename"), { at, title }),
  taskArchive: (at: TaskRef) => invoke<void>(octopus("task_archive"), { at }),
  /** Resolves to the created task's key. */
  taskCreate: (task: NewTask) => invoke<string>(octopus("task_create"), { task }),
  termAttach: (at: TaskRef, pty: PtyId, output: Channel<TermScreen | ArrayBuffer>) =>
    invoke<number>(octopus("term_attach"), { at, pty, output }),
  termInput: (attachment: number, keys: string) => invoke<void>(octopus("term_input"), { attachment, keys }),
  termDetach: (attachment: number) => invoke<void>(octopus("term_detach"), { attachment }),
  /** Resolves to the id of the shell panel it opens at `path`, relative to the task's directory. */
  termOpen: (at: TaskRef, path: string) => invoke<PtyId>(octopus("term_open"), { at, path }),
  termInstallMicrophone: (at: TaskRef) => invoke<PtyId>(octopus("term_install_microphone"), { at }),
  termClose: (at: TaskRef, panel: PtyId) => invoke<void>(octopus("term_close"), { at, panel }),
  termClaim: (at: TaskRef, panel: PtyId, cols: number, rows: number) =>
    invoke<void>(octopus("term_claim"), { at, panel, cols, rows }),
  termTakeLock: (at: TaskRef, panel: PtyId, cols: number, rows: number) =>
    invoke<void>(octopus("term_take_lock"), { at, panel, cols, rows }),
  termResize: (at: TaskRef, panel: PtyId, cols: number, rows: number) =>
    invoke<void>(octopus("term_resize"), { at, panel, cols, rows }),
  /** Resolves to the id of the panel that shows `url` on the task's server. */
  browserOpen: (at: TaskRef, panel: BrowserTarget, url: string) =>
    invoke<BrowserId>(octopus("browser_open"), { at, panel, url }),
  /** Removes the panel on the task's server, and so from every client. */
  browserClose: (at: TaskRef, panel: BrowserId) => invoke<void>(octopus("browser_close"), { at, panel }),
  /** Moves the panel to index `to` among the task's panels on its server, and so on every client. */
  browserMove: (at: TaskRef, panel: BrowserId, to: number) => invoke<void>(octopus("browser_move"), { at, panel, to }),
  fileList: (at: TaskRef, path: string) => invoke<Entry[]>(octopus("file_list"), { at, path }),
  /** The URL each file of the task's directory loads under, at its path with each part percent-encoded. */
  fileRootUrl: (at: TaskRef) => invoke<string>(octopus("file_root_url"), { at }),
  fileWrite: (at: TaskRef, path: string, content: string) => invoke<void>(octopus("file_write"), { at, path, content }),
  fileCreate: (at: TaskRef, path: string, kind: EntryKind) => invoke<void>(octopus("file_create"), { at, path, kind }),
  fileMove: (at: TaskRef, from: string, to: string) => invoke<void>(octopus("file_move"), { at, from, to }),
  fileCopy: (at: TaskRef, from: string, to: string) => invoke<void>(octopus("file_copy"), { at, from, to }),
  /** `trash` sends it to the trash of the server's machine; otherwise it is gone for good. */
  fileDelete: (at: TaskRef, path: string, trash: boolean) => invoke<void>(octopus("file_delete"), { at, path, trash }),
  /** Copies `local`, a file or folder on this machine, to the new `path`. */
  fileUpload: (at: TaskRef, path: string, local: string) => invoke<void>(octopus("file_upload"), { at, path, local }),
  /** Sends `content`, a file this client holds, as the new `path`. */
  fileUploadContent: async (at: TaskRef, path: string, content: Blob) =>
    invoke<void>(octopus("file_upload_content"), { at, path, content: await base64(content) }),
  fileSearch: (at: TaskRef, pattern: string) => invoke<Match[]>(octopus("file_search"), { at, pattern }),
};

export const on =
  <T>(event: string) =>
  (handle: (payload: T) => void) =>
    listen<T>(event, (heard) => handle(heard.payload));

export const events = {
  model: on<Model>(EVENTS.model),
  servers: on<ServerView[]>(EVENTS.servers),
  account: on<AccountView>(EVENTS.account),
  filesChanged: on<FilesChanged[]>(EVENTS.filesChanged),
  ui: on<Ui>(EVENTS.ui),
  windowShown: on<boolean>(EVENTS.windowShown),
  windowFocused: on<void>(EVENTS.windowFocused),
  fileDrag: on<FileDrag>(EVENTS.fileDrag),
  microphoneDenied: on<boolean>(EVENTS.microphoneDenied),
};

export { Channel };
