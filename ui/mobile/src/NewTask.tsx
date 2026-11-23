import { For, Show } from "solid-js";
import { MODES, TASK_STATES } from "@octopus/shared/generated.ts";
import { modeLabel, newTaskDraft, workspaceKey } from "@octopus/shared/newTaskDraft.ts";
import { serverLabel, workspaces } from "@octopus/shared/state.ts";
import { keepInView } from "@octopus/shared/keepInView.ts";
import { feel } from "./feel";
import { Icon } from "./Icon";
import { unfold } from "./motion";
import { openTask, raiseSheet, showInSheet } from "./nav";
import { Chev, Field, Flab, Gap, Note, Para, Secondary, sectionColor, Seg, Slot } from "./parts";
import { Sheet, SheetHead } from "./Sheet";

/**
 * Every field of `task.create` on one full sheet, with `section`'s workspace on its first reachable server chosen.
 * The folder opens a sheet over the form to type and browse it in. Create stays inert until the title, workspace and
 * harness are set and the server reads the folder as a directory; a refusal shows under the header.
 */
export function NewTask(props: { section: string | null }) {
  const draft = newTaskDraft(props.section);
  const create = () =>
    void draft.create().then((key) => {
      if (key === undefined) return;
      if (key === null) return feel("error");
      feel("success");
      openTask(key);
    });
  const pickFolder = () => {
    const picker = draft.pickFolder();
    if (picker !== undefined) raiseSheet({ is: "folder", picker });
  };
  const parentTitle = () => draft.parents().find((view) => view.task.id === draft.parent())?.task.title ?? "none";
  return (
    <Show
      when={workspaces().length > 0}
      fallback={
        <Sheet>
          <SheetHead title="New task" />
          <Para>
            No server lists a workspace yet. Add one with <b>+ workspace</b> at the end of Tasks.
          </Para>
          <Gap n={3} />
          <Secondary tap={() => showInSheet({ is: "addws" }, "deeper")}>Add a workspace</Secondary>
          <Gap n={2} />
        </Sheet>
      }
    >
      <Sheet full>
        <SheetHead
          title="New task"
          end={
            <button class="btn solid" classList={{ inert: !draft.complete() || draft.creating() }} onClick={create}>
              Create
            </button>
          }
        />
        <Show when={draft.refusal()}>
          {(refusal) => (
            <div ref={unfold} class="note err wrap">
              <Slot>
                <Icon name="warn" />
              </Slot>
              <span class="name">{refusal()}</span>
            </div>
          )}
        </Show>
        <Flab>title</Flab>
        <Field value={draft.title()} input={draft.setTitle} focus />
        <Flab>workspace</Flab>
        <div class="pick">
          <For each={workspaces()}>
            {(option) => {
              const reachable = () => draft.reachable(option);
              return (
                <span
                  ref={keepInView(() => workspaceKey(option) === draft.workspace())}
                  class="chip"
                  role="button"
                  classList={{ on: workspaceKey(option) === draft.workspace(), off: !reachable() }}
                  onClick={() => {
                    if (!reachable() || workspaceKey(option) === draft.workspace()) return;
                    feel("select");
                    draft.setWorkspace(workspaceKey(option));
                  }}
                >
                  <i class="dot srv" style={{ background: sectionColor(option.name) }} />
                  {option.name} · {serverLabel(option.server)}
                </span>
              );
            }}
          </For>
        </div>
        <Flab>folder</Flab>
        <div
          class="field mono folder"
          classList={{ off: draft.chosen() === undefined }}
          role="button"
          aria-label="Folder"
          onClick={pickFolder}
        >
          <Slot>
            <Icon name="folder" />
          </Slot>
          <span class="path">
            <bdi>
              <span class="root">{draft.lead()}</span>
              {draft.path()}
            </bdi>
          </span>
          <Show when={draft.tag()}>{(tag) => <span class="id tag">{tag()}</span>}</Show>
          <Chev />
        </div>
        <Flab>mode</Flab>
        <Seg
          wide
          options={MODES.map((mode) => [mode, modeLabel(mode)] as const)}
          value={draft.mode()}
          choose={draft.setMode}
          off={draft.modeOff}
        />
        <Flab>parent</Flab>
        <label class="field">
          <span class="name">{parentTitle()}</span>
          <Chev />
          <select
            class="native"
            value={draft.parent() ?? ""}
            onChange={(event) => draft.setParent(event.currentTarget.value || null)}
          >
            <option value="">none</option>
            <For each={draft.parents()}>{(view) => <option value={view.task.id}>{view.task.title}</option>}</For>
          </select>
        </label>
        <Flab>state</Flab>
        <Seg
          wide
          options={TASK_STATES.map((each) => [each, each] as const)}
          value={draft.state()}
          choose={draft.setState}
        />
        <Flab>harness</Flab>
        <label class="field">
          <span class="name">{draft.harness()}</span>
          <Chev />
          <select
            class="native"
            value={draft.harness()}
            onChange={(event) => draft.setHarness(event.currentTarget.value)}
          >
            <For each={draft.harnesses()}>{(id) => <option value={id}>{id}</option>}</For>
          </select>
        </label>
        <Show when={draft.harnessMissing()}>
          {(missing) => (
            <Note err wrap>
              {missing()}
            </Note>
          )}
        </Show>
        <Flab>session</Flab>
        <Field value={draft.session()} input={draft.setSession} placeholder="fresh session, or an id to resume" />
        <Gap n={2} />
      </Sheet>
    </Show>
  );
}
