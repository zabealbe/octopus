import { createSignal, For, Match, onMount, Show, Switch } from "solid-js";
import type { Appearance, DeviceView, ShownLink } from "./generated";
import { backend } from "./backend";
import { commands, hint } from "./commands";
import {
  closeOnLastingBlur,
  closePopover,
  Confirm,
  copyKey,
  copyOnShortcut,
  dismissible,
  Head,
  Item,
  Sep,
} from "./Menu";
import { expiry } from "./pairing";
import { ServerMenu } from "./ServerMenu";
import {
  account,
  act,
  harnesses,
  me,
  report,
  servers,
  setAgentFlags,
  setAppearance,
  showClientMenu,
  showModal,
  ui,
  version,
  zoom,
} from "./state";
import { ticking } from "./time";
import { copy } from "./Toast";

export const APPEARANCES: [Appearance, string][] = [
  ["system", "System"],
  ["light", "Light"],
  ["dark", "Dark"],
];
const COPIED_MS = 1800;

/** "About octopus" copies "octopus <version>" and reads "copied" for a moment after. */
export function aboutCopy() {
  const [copied, setCopied] = createSignal(false);
  const copy = () =>
    navigator.clipboard.writeText(`octopus ${version()}`).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), COPIED_MS);
    }, report);
  return { copied, copy };
}

export const NEXT_START = "Applies on each agent's next start.";

/** Enter saves the flags typed, and leaving the field puts the saved ones back. */
function AgentFlagsField(props: { harness: string }) {
  const saved = () => account().settings.agent_flags[props.harness] ?? "";
  return (
    <label class="field">
      <span>{props.harness}</span>
      <input
        class="mono"
        spellcheck={false}
        placeholder="none"
        value={saved()}
        onKeyDown={(event) => {
          if (event.key !== "Enter") return;
          const input = event.currentTarget;
          setAgentFlags(props.harness, input.value).then(() => input.blur(), report);
        }}
        onBlur={(event) => (event.currentTarget.value = saved())}
      />
    </label>
  );
}

export const REMOVE_CONFIRM = (name: string) => `Remove ${name}? It loses every server of this account.`;
export const LEAVE_CONFIRM = (device: "client" | "phone") =>
  `Leave this account? This ${device} keeps its servers; the other devices keep theirs.`;

type AccountMode = "main" | "leave" | { remove: DeviceView } | ShownLink;

/** What the account section shows in place: its devices, a shown link, or a removal or leave being confirmed. */
export function accountModes() {
  const [mode, setMode] = createSignal<AccountMode>("main");
  const shownLink = () => {
    const shown = mode();
    return typeof shown === "object" && "link" in shown ? shown : null;
  };
  const removing = () => {
    const shown = mode();
    return typeof shown === "object" && "remove" in shown ? shown.remove : null;
  };
  return { mode, setMode, shownLink, removing, done: () => setMode("main") };
}

function Account() {
  const now = ticking();
  const { mode, setMode, shownLink, removing, done } = accountModes();
  const copyLink = (shown: ShownLink) => act(copy(shown.link, "the link"));
  copyOnShortcut(shownLink, copyLink);
  const Join = () => <Item run={() => showModal("join_account")}>Join an account…</Item>;
  return (
    <>
      <div class="mh label">
        <span>Devices</span>
      </div>
      <Show
        when={account().member}
        fallback={
          <>
            <div class="mh label">Removed from this account</div>
            <Join />
            <Item run={() => act(backend.accountStartOver())}>Start over</Item>
          </>
        }
      >
        <For each={account().devices}>
          {(device) => (
            <div class="client">
              <span class="name">{device.name}</span>
              <Show
                when={device.this}
                fallback={
                  <button class="mini" onClick={() => setMode({ remove: device })}>
                    Remove
                  </button>
                }
              >
                <span class="mono">this client</span>
              </Show>
            </div>
          )}
        </For>
        <Show when={removing()}>
          {(device) => (
            <Confirm act="Remove" run={() => backend.accountRemove(device().key).then(done, report)} cancel={done}>
              {REMOVE_CONFIRM(device().name)}
            </Confirm>
          )}
        </Show>
        <Show when={shownLink()}>
          {(shown) => (
            <>
              <div class="code">
                <div class="qr" innerHTML={shown().qr} />
                <div class="exp">{expiry(shown().expires_at, now(), "one device")}</div>
              </div>
              <Item kbd={copyKey} run={() => copyLink(shown())}>
                Copy link
              </Item>
            </>
          )}
        </Show>
        <Item run={() => backend.accountLink().then(setMode, report)}>
          <span class="glyph">+</span>Link a device…
        </Item>
        <Switch>
          <Match when={account().devices.length < 2}>
            <Join />
          </Match>
          <Match when={mode() === "leave"}>
            <Confirm act="Leave" run={() => backend.accountLeave().then(done, report)} cancel={done}>
              {LEAVE_CONFIRM("client")}
            </Confirm>
          </Match>
          <Match when={true}>
            <Item danger run={() => setMode("leave")}>
              Leave account
            </Item>
          </Match>
        </Switch>
      </Show>
    </>
  );
}

/** Hangs from the title bar's gear, and shows a server's menu in its place once that server is chosen. */
export function ClientMenu(props: { floats: (element: HTMLElement) => void; shown: boolean }) {
  let element!: HTMLDivElement;
  const [server, setServer] = createSignal<string | null>(null);
  const { copied, copy: about } = aboutCopy();
  onMount(() => {
    closePopover();
    dismissible(element, () => showClientMenu(false));
    closeOnLastingBlur(() => showClientMenu(false));
  });
  return (
    <div
      ref={(menu) => {
        element = menu;
        props.floats(menu);
      }}
      class="menu wide client-menu"
      classList={{ hidden: !props.shown }}
      role="menu"
    >
      <Show
        when={server()}
        keyed
        fallback={
          <>
            <Head title={me()} lines={[`account ${account().id}`]} />
            <Account />
            <Sep />
            <div class="mh label">
              <span>Servers</span>
            </div>
            <For each={servers()}>
              {(view) => (
                <button class="mi" role="menuitem" onClick={() => setServer(view.name)}>
                  <span class="dot" classList={{ off: !view.reachable }} />
                  <span class="name">{view.label}</span>
                  <span class="rtt">{view.reachable ? `${view.rtt_ms ?? "–"} ms` : "unreachable"}</span>
                </button>
              )}
            </For>
            <Item run={() => commands.pairServer.run()}>
              <span class="glyph">+</span>Pair a server…
            </Item>
            <Sep />
            <div class="mrow">
              Appearance
              <div class="seg">
                <For each={APPEARANCES}>
                  {([appearance, label]) => (
                    <button
                      classList={{ on: ui.prefs.appearance === appearance }}
                      onClick={() => setAppearance(appearance)}
                    >
                      {label}
                    </button>
                  )}
                </For>
              </div>
            </div>
            <div class="mrow">
              Zoom
              <span class="stepper">
                <button class="mini" title={hint(commands.zoomOut)} onClick={() => zoom("out")}>
                  −
                </button>
                <output>{ui.prefs.zoom}%</output>
                <button class="mini" title={hint(commands.zoomIn)} onClick={() => zoom("in")}>
                  +
                </button>
              </span>
            </div>
            <Show when={harnesses().length > 0}>
              <Sep />
              <div class="mh label">
                <span>Agent flags</span>
              </div>
              <For each={harnesses()}>{(harness) => <AgentFlagsField harness={harness} />}</For>
              <div class="mh label">{NEXT_START}</div>
            </Show>
            <Sep />
            <Item kbd={hint(commands.shortcuts)} run={commands.shortcuts.run}>
              Keyboard shortcuts
            </Item>
            <button class="mi" role="menuitem" onClick={about}>
              About octopus<span class="why">{copied() ? "copied" : version()}</span>
            </button>
            <Item kbd={hint(commands.quit)} run={commands.quit.run}>
              Quit octopus
            </Item>
          </>
        }
      >
        {(name) => <ServerMenu name={name} />}
      </Show>
    </div>
  );
}
