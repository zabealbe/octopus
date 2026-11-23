import { createSignal, For, Match, Show, Switch, type JSX } from "solid-js";
import { backend } from "@octopus/shared/backend.ts";
import {
  aboutCopy,
  accountModes,
  APPEARANCES,
  LEAVE_CONFIRM,
  NEXT_START,
  REMOVE_CONFIRM,
} from "@octopus/shared/ClientMenu.tsx";
import { ZOOMS, type PairedAccount, type ShownOffer } from "@octopus/shared/generated.ts";
import {
  accountLabel,
  clientStatus,
  expiry,
  firstWhere,
  grouped,
  inviteCode,
  isThisAccount,
  isThisClient,
  pairedAccounts,
} from "@octopus/shared/pairing.ts";
import { clock, ticking } from "@octopus/shared/time.ts";
import {
  account,
  act,
  harnesses,
  me,
  report,
  serverView,
  servers,
  setAgentFlags,
  setAppearance,
  ui,
  version,
  zoom,
} from "@octopus/shared/state.ts";
import { shortKey } from "./format";
import { feel } from "./feel";
import { Icon } from "./Icon";
import { go, showInSheet } from "./nav";
import { Chev, Confirm, Field, Flab, Gap, Gl, Id, Mi, Note, Seg, Sep, ServerDot, Why } from "./parts";
import { NameField } from "./NameField";
import { Sheet, SheetHead } from "./Sheet";
import { copy } from "./Toast";

const toMenu = () => showInSheet({ is: "client" }, "back");

const rtt = (ms: number | null) => `${ms ?? "–"} ms`;

/** Each zoom step ticks, and a step past the end gives the OS's warning instead. */
const step = (to: "in" | "out") => {
  const at = (ZOOMS as readonly number[]).indexOf(ui.prefs.zoom);
  feel((to === "in" ? at === ZOOMS.length - 1 : at === 0) ? "warning" : "select");
  zoom(to);
};

/** Return saves the flags typed, and a refusal shows under the field. */
function AgentFlagsField(props: { harness: string }) {
  const [draft, setDraft] = createSignal(account().settings.agent_flags[props.harness] ?? "");
  const [problem, setProblem] = createSignal<string | null>(null);
  const save = () =>
    setAgentFlags(props.harness, draft()).then(
      () => {
        feel("success");
        setProblem(null);
      },
      (error) => {
        feel("error");
        setProblem(String(error));
      },
    );
  return (
    <>
      <Flab>{props.harness}</Flab>
      <Field mono value={draft()} input={setDraft} placeholder="none" enter={save} />
      <Show when={problem()}>{(shown) => <Note err>{shown()}</Note>}</Show>
    </>
  );
}

const copyText = (text: string, what: string) => act(copy(text, `the ${what}`));
const shareText = (text: string, what: string) =>
  navigator.share
    ? navigator.share({ text }).catch((error: Error) => {
        if (error.name !== "AbortError") report(error);
      })
    : copyText(text, what);

/** A QR code of `text`, then Copy and Share, which hands `text` to the OS's share sheet. */
function Shared(props: { qr: string; text: string; what: string; copy: string; children?: JSX.Element }) {
  return (
    <>
      <div class="qr" innerHTML={props.qr} />
      <Gap n={3} />
      {props.children}
      <div class="pair2">
        <button class="secondary" onClick={() => copyText(props.text, props.what)}>
          <Icon name="copy" />
          {props.copy}
        </button>
        <button class="secondary" onClick={() => shareText(props.text, props.what)}>
          <Icon name="share" />
          Share
        </button>
      </div>
      <Gap n={2} />
    </>
  );
}

const JoinAccount = () => (
  <Mi slot={<Icon name="cam" />} end={<Chev />} tap={() => go({ at: "scan", reading: "link" })}>
    Join an account…
  </Mi>
);

export function ClientMenu() {
  const now = ticking();
  const { copied, copy: about } = aboutCopy();
  const { mode, setMode, shownLink, removing, done } = accountModes();
  const succeed = () => {
    feel("success");
    done();
  };
  return (
    <Sheet>
      <Show when={shownLink()}>
        {(shown) => (
          <>
            <SheetHead title="Link a device" facts={[expiry(shown().expires_at, now(), "one device")]} back={done} />
            <Shared qr={shown().qr} text={shown().link} what="link" copy="Copy link" />
          </>
        )}
      </Show>
      <Show when={shownLink() === null}>
        <SheetHead title={me()} facts={[`account ${account().id}`]} />
        <Gl>devices</Gl>
        <Show
          when={account().member}
          fallback={
            <>
              <Note>Removed from this account</Note>
              <JoinAccount />
              <Mi slot={<Icon name="plus" />} tap={() => backend.accountStartOver().then(succeed, report)}>
                Start over
              </Mi>
            </>
          }
        >
          <For each={account().devices}>
            {(device) => (
              <Mi
                end={
                  <Show
                    when={device.this}
                    fallback={
                      <button class="btn quiet" onClick={() => setMode({ remove: device })}>
                        Remove
                      </button>
                    }
                  >
                    <Why>this phone</Why>
                  </Show>
                }
              >
                {device.name}
              </Mi>
            )}
          </For>
          <Show when={removing()}>
            {(device) => (
              <Confirm
                text={REMOVE_CONFIRM(device().name)}
                act="Remove"
                run={() => backend.accountRemove(device().key).then(succeed, report)}
                cancel={done}
              />
            )}
          </Show>
          <Mi slot={<Icon name="qr" />} end={<Chev />} tap={() => backend.accountLink().then(setMode, report)}>
            Link a device…
          </Mi>
          <Switch>
            <Match when={account().devices.length < 2}>
              <JoinAccount />
            </Match>
            <Match when={mode() === "leave"}>
              <Confirm
                text={LEAVE_CONFIRM("phone")}
                act="Leave"
                run={() => backend.accountLeave().then(succeed, report)}
                cancel={done}
              />
            </Match>
            <Match when={true}>
              <Mi
                slot={<Icon name="x" />}
                danger
                tap={() => {
                  feel("warning");
                  setMode("leave");
                }}
              >
                Leave account
              </Mi>
            </Match>
          </Switch>
        </Show>
        <Gl>servers</Gl>
        <For each={servers()}>
          {(server) => (
            <Mi
              class={server.reachable ? undefined : "off"}
              slot={<ServerDot server={server} />}
              end={
                <>
                  <Id>{server.reachable ? rtt(server.rtt_ms) : "unreachable"}</Id>
                  <Chev />
                </>
              }
              tap={() => showInSheet({ is: "server", name: server.name }, "deeper")}
            >
              {server.label}
            </Mi>
          )}
        </For>
        <Mi slot={<Icon name="plus" />} tap={() => go({ at: "scan", reading: "invite" })}>
          Pair a server…
        </Mi>
        <Sep />
        <Mi
          slot={<Icon name="sun" />}
          end={<Seg options={APPEARANCES} value={ui.prefs.appearance} choose={setAppearance} />}
        >
          Appearance
        </Mi>
        <Mi
          slot={<Icon name="zoom" />}
          end={
            <div class="stepper">
              <span role="button" aria-label="Zoom out" onClick={() => step("out")}>
                <Icon name="minus" />
              </span>
              <output>{ui.prefs.zoom}%</output>
              <span role="button" aria-label="Zoom in" onClick={() => step("in")}>
                <Icon name="plus" />
              </span>
            </div>
          }
        >
          Zoom
        </Mi>
        <Show when={harnesses().length > 0}>
          <Gl>agent flags</Gl>
          <For each={harnesses()}>{(harness) => <AgentFlagsField harness={harness} />}</For>
          <Note>{NEXT_START}</Note>
        </Show>
        <Sep />
        <Mi slot={<Icon name="info" />} end={<Why>{copied() ? "copied" : version()}</Why>} tap={about}>
          About octopus
        </Mi>
        <Gap n={2} />
      </Show>
    </Sheet>
  );
}

export function ServerMenu(props: { name: string }) {
  const server = () => serverView(props.name);
  const now = ticking();
  const [mode, setMode] = createSignal<"main" | "rename" | "forget" | ShownOffer>("main");
  const offer = () => {
    const shown = mode();
    return typeof shown === "string" ? null : shown;
  };
  const { listed, note, refetch } = pairedAccounts(props.name);
  const facts = () => {
    const shown = server();
    if (shown === undefined) return [];
    const reach = shown.reachable
      ? `reachable · ${rtt(shown.rtt_ms)}`
      : `unreachable since ${clock(shown.since, "h23")}`;
    return [reach, shown.key === null ? "local socket" : `key ${shortKey(shown.key)}`];
  };
  const showCode = () => backend.serverOffer(props.name).then(setMode, report);
  const revoke = (paired: PairedAccount) =>
    backend.serverRevoke(props.name, paired.account).then(() => {
      feel("success");
      refetch();
    }, report);
  const rename = (name: string) =>
    backend.serverRename(props.name, name).then(() => showInSheet({ is: "server", name }, "across"));
  const forget = () =>
    backend.serverForget(props.name).then(() => {
      feel("success");
      toMenu();
    }, report);
  const head = (lines: string[], up: () => void) => <SheetHead title={props.name} facts={lines} back={up} />;
  return (
    <Sheet>
      <Switch>
        <Match when={offer()}>
          {(offered) => (
            <>
              {head([expiry(offered().offer.expires_at, now(), "one pairing")], () => setMode("main"))}
              <Shared qr={offered().qr} text={offered().offer.invite} what="pairing invite" copy="Copy invite">
                <div class="cmd">
                  <span class="name">{grouped(inviteCode(offered().offer))}</span>
                </div>
                <Gap n={3} />
              </Shared>
            </>
          )}
        </Match>
        <Match when={true}>
          {head(facts(), toMenu)}
          <Mi
            slot={<Icon name="qr" />}
            off={!server()?.reachable}
            end={server()?.reachable ? <Chev /> : <Why>unreachable</Why>}
            tap={showCode}
          >
            Show pairing code
          </Mi>
          <Show when={mode() !== "forget"}>
            <Show
              when={listed()}
              fallback={
                <>
                  <Gl>paired accounts</Gl>
                  <div class="empty">{note()}</div>
                </>
              }
            >
              {(shown) => (
                <For each={shown()}>
                  {(paired) => (
                    <>
                      <Gl>{accountLabel(paired)}</Gl>
                      <For each={firstWhere(paired.devices, isThisClient)}>
                        {(client) => (
                          <Mi
                            slot={<i class={client.connected || client.hosts ? "dot srv" : "dot ring"} />}
                            end={<Why>{clientStatus(client, now(), "this phone")}</Why>}
                          >
                            {client.name}
                          </Mi>
                        )}
                      </For>
                      <Show when={!isThisAccount(paired)}>
                        <Mi slot={<Icon name="x" />} danger tap={() => revoke(paired)}>
                          Revoke account
                        </Mi>
                      </Show>
                    </>
                  )}
                </For>
              )}
            </Show>
          </Show>
          <Sep />
          <Show
            when={mode() === "rename"}
            fallback={
              <Mi slot={<Icon name="pen" />} tap={() => setMode("rename")}>
                Rename…
              </Mi>
            }
          >
            <NameField value={props.name} apply={rename} done={() => setMode("main")} />
          </Show>
          <Show
            when={mode() === "forget"}
            fallback={
              <Mi
                slot={<Icon name="x" />}
                danger
                tap={() => {
                  feel("warning");
                  setMode("forget");
                }}
              >
                Forget server
              </Mi>
            }
          >
            <Gap n={2} />
            <Confirm
              text={`Forget ${props.name}? Its tasks leave this account's lists; they keep running on ${props.name}. Pairing again needs a new code.`}
              act="Forget"
              run={forget}
              cancel={() => setMode("main")}
            />
          </Show>
          <Gap n={2} />
        </Match>
      </Switch>
    </Sheet>
  );
}
