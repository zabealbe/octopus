import { createEffect, createMemo, createResource, createSignal, on } from "solid-js";
import { backend } from "./backend";
import { folderPicker, prefixOf, readFolder, readingOf, relative, repoTag } from "./folderPath";
import { FRESH_SESSION, type Folder, type Mode, type TaskState, type WorkspaceRef } from "./generated";
import { model, openView, serverLabel, serverView, workspaces } from "./state";

export const workspaceKey = (workspace: WorkspaceRef) => `${workspace.server}/${workspace.workspace}`;

export const modeLabel = (mode: Mode) => mode.replace("_", " ");

/**
 * The fields of a new task, each at its start: the workspace is `section`'s on its first reachable server, else the
 * open task's, and the folder is the workspace's root until the user picks one. The folder is read through
 * `folder.get` as it changes; the draft is complete once the title, workspace and harness are set and the server has
 * read the folder as a directory.
 */
export function newTaskDraft(section: string | null) {
  const current = openView();
  const reachable = (workspace: WorkspaceRef) => serverView(workspace.server)?.reachable === true;
  const choices = () => workspaces().filter(reachable);
  const initial =
    choices().find((workspace) => workspace.name === section) ??
    choices().find((workspace) => workspace.server === current?.at.server && workspace.name === current.section);
  const [workspace, chooseWorkspace] = createSignal(initial === undefined ? "" : workspaceKey(initial));
  const chosen = () => choices().find((candidate) => workspaceKey(candidate) === workspace());
  const [title, setTitle] = createSignal("");
  const root = () => chosen()?.root ?? "/";
  /** The folder picked, absolute; none while the folder is the workspace's root. */
  const [picked, pick] = createSignal<string | null>(null);
  const folder = () => picked() ?? root();
  createEffect(() => {
    const on = chosen()?.server;
    if (on !== undefined) readFolder(on, folder());
  });
  /** The server's reading of the folder: the folder, or why it is none; undefined while it is read. */
  const reading = () => {
    const on = chosen()?.server;
    return on === undefined ? undefined : readingOf(on, folder());
  };
  /** The folder, once the server has read it as a directory. */
  const shown = () => {
    const now = reading();
    return typeof now === "object" ? now : undefined;
  };
  /** The folder as its field shows it: relative to the root under it, else absolute. */
  const path = () => relative(folder(), root());
  /** What the field shows dimmed before the path: the root, with a slash while a path follows it. */
  const lead = () => (path() === "" ? root() : prefixOf(path(), root()));
  /** The last folder the server read as a directory, which a folder still being read keeps. */
  const lastRead = createMemo<Folder | undefined>((was) => {
    const now = reading();
    return now === undefined ? was : typeof now === "object" ? now : undefined;
  }, undefined);
  const tag = () => {
    const now = lastRead();
    return now === undefined ? null : repoTag(now);
  };
  /** Opens a picker on the folder, which hands the folder it chooses back to the draft. */
  const pickFolder = () => {
    const on = chosen();
    return on === undefined ? undefined : folderPicker(on.server, on.root, path(), pick);
  };
  const [mode, setMode] = createSignal<Mode>("in_place");
  const repo = createMemo(() => lastRead()?.repo ?? null);
  createEffect(on(repo, (_, was) => was !== null && setMode("in_place"), { defer: true }));
  /** Whether `mode` is out of reach: worktree and copy need the folder inside a git repo. */
  const modeOff = (each: Mode) => each !== "in_place" && repo() === null;
  const [parent, setParent] = createSignal<string | null>(null);
  const parents = () =>
    Object.values(model.tasks).filter(
      (view) =>
        view.live &&
        view.at.server === chosen()?.server &&
        view.task.workspace === chosen()?.workspace &&
        view.task.archived_at === null,
    );
  const [state, setState] = createSignal<TaskState>("temporary");
  const [harness, setHarness] = createSignal(current?.task.harness ?? "claude-code");
  const [listed] = createResource(
    () => chosen()?.server ?? false,
    (on) => backend.harnessList(on).then((list) => list.harnesses),
  );
  /** The harnesses the workspace's server lists, led by the chosen one while the server lacks it. */
  const harnesses = () => {
    const known = listed.state === "errored" ? [] : (listed.latest ?? []);
    return known.includes(harness()) ? known : [harness(), ...known];
  };
  /** The harness checked on the workspace's server, as each of them changes. */
  const [checked] = createResource(
    () => {
      const [on, id] = [chosen()?.server, harness()];
      return on === undefined || id === "" ? false : { on, id };
    },
    ({ on, id }) => backend.harnessCheck(on, id).then((status) => ({ on, id, status })),
  );
  /** What the form says while the harness cannot start its agent on the workspace's server. */
  const harnessMissing = () => {
    const check = checked.state === "errored" ? undefined : checked.latest;
    if (check === undefined) return null;
    const server = serverLabel(check.on);
    if (check.status === null) return `${server} has no harness ${check.id}`;
    if (check.status.installed) return null;
    return `${check.status.program} is not installed on ${server}. Install it with ${check.status.install}`;
  };
  const [session, setSession] = createSignal("");
  const [refusal, setRefusal] = createSignal<string | null>(null);
  const [creating, setCreating] = createSignal(false);
  const complete = () => chosen() !== undefined && title().trim() !== "" && harness() !== "" && shown() !== undefined;

  const setWorkspace = (key: string) => {
    if (key === workspace()) return;
    setParent(null);
    chooseWorkspace(key);
  };
  /**
   * Resolves to the new task's key, to null once the server's refusal shows in the form, or to undefined at once while
   * the form is incomplete or already creating. A refusal shown stays until the next answer replaces it.
   */
  const create = async () => {
    const target = chosen();
    const listed = shown();
    if (target === undefined || listed === undefined || !complete() || creating()) return undefined;
    setCreating(true);
    const fresh = session().trim();
    return backend
      .taskCreate({
        server: target.server,
        workspace: target.workspace,
        folder: listed.folder,
        mode: mode(),
        title: title().trim(),
        parent: parent(),
        state: state(),
        harness: harness(),
        session: fresh === "" ? FRESH_SESSION : fresh,
      })
      .then(
        (key) => {
          setRefusal(null);
          return key;
        },
        (refused) => {
          setRefusal(String(refused));
          return null;
        },
      )
      .finally(() => setCreating(false));
  };

  return {
    reachable,
    workspace,
    setWorkspace,
    chosen,
    title,
    setTitle,
    path,
    lead,
    tag,
    pickFolder,
    mode,
    setMode,
    modeOff,
    parent,
    setParent,
    parents,
    state,
    setState,
    harness,
    setHarness,
    harnesses,
    harnessMissing,
    session,
    setSession,
    refusal,
    creating,
    complete,
    create,
  };
}
