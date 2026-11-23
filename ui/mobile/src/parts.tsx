import { For, onCleanup, Show, type JSX } from "solid-js";
import { animateList } from "@octopus/shared/flip.ts";
import type { Glyph as Status, ServerView } from "@octopus/shared/generated.ts";
import { turning } from "@octopus/shared/Glyph.tsx";
import { keepInView } from "@octopus/shared/keepInView.ts";
import type { Within } from "@octopus/shared/lift.ts";
import { createArrangement, liftTab } from "@octopus/shared/liftTab.ts";
import { sectionView } from "@octopus/shared/state.ts";
import type { Size } from "@octopus/shared/terminalSession.ts";
import { swatchColor } from "@octopus/shared/tokens.ts";
import { feel } from "./feel";
import { settles } from "./front";
import { Icon, type IconName } from "./Icon";
import { onPainted } from "./motion";
import { back } from "./nav";
import { liftOnPhone, pressed } from "./press";
import { slidingThumb } from "./thumb";

/** Calls `set` with `element`'s size each time it changes, for as long as the calling component lives. */
export function observeSize(element: HTMLElement, set: (size: Size) => void) {
  const observer = new ResizeObserver(() => set({ width: element.clientWidth, height: element.clientHeight }));
  observer.observe(element);
  onCleanup(() => observer.disconnect());
}

/** The color of the section named `name`, faint while the model lists none by that name. */
export const sectionColor = (name: string | undefined) => {
  const section = sectionView(name);
  return section === undefined ? "var(--faint)" : swatchColor(section.swatch);
};

/** A 20pt slot holding a glyph, caret or icon, centered on its text's line box. */
export const Slot = (props: { children?: JSX.Element; class?: string }) => (
  <span class={props.class ? `slot ${props.class}` : "slot"}>{props.children}</span>
);

export const Circle = (props: { icon: IconName; label: string; on?: boolean; tap: () => void }) => (
  <button class="circle" classList={{ on: props.on }} aria-label={props.label} onClick={() => props.tap()}>
    <Icon name={props.icon} big />
  </button>
);

const look: Record<Status, () => JSX.Element> = {
  working: () => (
    <svg class="spin" viewBox="0 0 12 12">
      <circle cx="6" cy="6" r="5" pathLength="100" ref={turning} />
    </svg>
  ),
  waiting: () => <i class="dot needs" />,
  idle: () => <i class="dot idle" />,
  asleep: () => <i class="dot ring" />,
  ended: () => <i class="dot ring" />,
  waking: () => (
    <svg class="spin wake" viewBox="0 0 12 12">
      <circle cx="6" cy="6" r="5" pathLength="100" ref={turning} />
    </svg>
  ),
  completed: () => (
    <span class="end">
      <Icon name="check" />
    </span>
  ),
  cancelled: () => (
    <span class="end">
      <Icon name="x" />
    </span>
  ),
};

/** A task's status: working spins, waiting is amber, idle a grey dot, an agent not running a ring, waking a faint spinner, and an ended task its check or cross. */
export const Glyph = (props: { status: Status }) => <>{look[props.status]()}</>;

export const looksReachable = (server: ServerView | undefined) =>
  server !== undefined && (server.reachable || settles(server.name));

/** A server's dot: filled while it looks reachable, hollow while not, an amber ring while the phone reconnects to it. */
export const ServerDot = (props: { server: ServerView | undefined; reconnecting?: boolean }) => (
  <i
    class="dot srv"
    classList={{ off: !looksReachable(props.server) && !props.reconnecting, re: props.reconnecting }}
  />
);

export const Id = (props: { children: JSX.Element; class?: string }) => (
  <span class={props.class ? `id ${props.class}` : "id"}>{props.children}</span>
);

/** A path cut from its start when it runs past the right rail, so its tail stays. */
export const Path = (props: { children: string }) => (
  <span class="path">
    <bdi>{props.children}</bdi>
  </span>
);

export const Why = (props: { children: JSX.Element }) => <span class="why">{props.children}</span>;

export const Chev = () => (
  <span class="chev">
    <Icon name="chev" />
  </span>
);

/** A 48pt menu item: its icon or glyph in the slot, its label, and its value, reason or chevron on the right rail. */
export function Mi(props: {
  slot?: JSX.Element;
  children: JSX.Element;
  end?: JSX.Element;
  class?: string;
  off?: boolean;
  danger?: boolean;
  tap?: () => void;
}) {
  return (
    <div
      class={props.class ? `mi ${props.class}` : "mi"}
      classList={{ off: props.off, danger: props.danger }}
      role="button"
      aria-disabled={props.off ? "true" : undefined}
      onClick={() => !props.off && props.tap?.()}
    >
      <Slot>{props.slot}</Slot>
      <span class="name">{props.children}</span>
      {props.end}
    </div>
  );
}

/** A destructive step's confirmation, in place on its red block. */
export function Confirm(props: { text: string; act: string; run: () => void; cancel: () => void }) {
  return (
    <div class="confirm">
      <span>{props.text}</span>
      <div class="row2">
        <button class="btn quiet" onClick={props.cancel}>
          Cancel
        </button>
        <button class="btn danger" onClick={props.run}>
          {props.act}
        </button>
      </div>
    </div>
  );
}

export const Sep = () => <div class="sep" />;
export const Gl = (props: { children: JSX.Element }) => <div class="gl">{props.children}</div>;
export const Flab = (props: { children: JSX.Element }) => <div class="flab">{props.children}</div>;
export const Para = (props: { children: JSX.Element; center?: boolean }) => (
  <div class="para" classList={{ c: props.center }}>
    {props.children}
  </div>
);
export const Note = (props: { children: JSX.Element; err?: boolean; wrap?: boolean }) => (
  <div class="note" classList={{ err: props.err, wrap: props.wrap }}>
    <Show when={props.err}>
      <Slot>
        <Icon name="warn" />
      </Slot>
    </Show>
    <span class="name">{props.children}</span>
  </div>
);
export const Gap = (props: { n: 2 | 3 | 4 | 5 }) => <div class={`sp${props.n}`} />;

/**
 * A segmented control, the current segment on the surface under a thumb that slides to each choice. A segment `off`
 * says is dim and takes no tap, keeping its place.
 */
export function Seg<T extends string>(props: {
  options: readonly (readonly [T, string])[];
  value: T;
  choose: (value: T) => void;
  wide?: boolean;
  off?: (value: T) => boolean;
}) {
  return (
    <div class="seg" classList={{ wide: props.wide }} ref={slidingThumb}>
      <For each={props.options}>
        {([value, label]) => {
          const off = () => props.off?.(value) ?? false;
          return (
            <span
              role="button"
              classList={{ on: props.value === value, off: off() }}
              aria-disabled={off() ? "true" : undefined}
              onClick={() => {
                if (off()) return;
                if (props.value !== value) feel("select");
                props.choose(value);
              }}
            >
              {label}
            </span>
          );
        }}
      </For>
    </div>
  );
}

/** A 48pt filled field typing into its input, which the stylesheet rings while it has the focus. */
export function Field(props: {
  value: string;
  input: (value: string) => void;
  placeholder?: string;
  mono?: boolean;
  focus?: boolean;
  enter?: () => void;
}) {
  let input!: HTMLInputElement;
  onPainted(() => props.focus && input.focus({ preventScroll: true }));
  return (
    <label class="field" classList={{ mono: props.mono }}>
      <input
        class="name"
        value={props.value}
        placeholder={props.placeholder}
        spellcheck={false}
        autocapitalize="off"
        autocorrect="off"
        autocomplete="off"
        enterkeyhint="done"
        ref={input}
        onInput={(event) => props.input(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key !== "Enter") return;
          event.preventDefault();
          event.currentTarget.blur();
          props.enter?.();
        }}
      />
    </label>
  );
}

export const Primary = (props: { children: JSX.Element; tap: () => void; off?: boolean }) => (
  <button class="primary" disabled={props.off} onClick={() => props.tap()}>
    {props.children}
  </button>
);

export const Secondary = (props: { children: JSX.Element; tap: () => void }) => (
  <button class="secondary" onClick={() => props.tap()}>
    {props.children}
  </button>
);

export const Trail = (props: { parts: string[] }) => (
  <For each={props.parts}>
    {(part, nth) => (
      <>
        <Show when={nth() > 0}>
          <span class="sep">/</span>
        </Show>
        {part}
      </>
    )}
  </For>
);

export type FolderTab = { key: string; name: string; hint?: string; dirty?: boolean; loading?: boolean };

const IN_TABS: Within = { stage: ".tabs", list: ".tabrow", free: false };

/**
 * A row of folder tabs under a screen's bar, the selected one kept in view: each tab's spinner while its page loads,
 * its name, its unsaved dot and the hint that tells it from a tab of the same name, and its × to close it, then the +
 * `add` makes, when it is given. A tap chooses a tab, and a long press lifts it, and dragging it along the row, which
 * scrolls at its edges, moves it there.
 */
export function FolderTabs(props: {
  tabs: FolderTab[];
  selected: string;
  choose: (key: string) => void;
  close: (key: string) => void;
  move: (key: string, to: number) => Promise<unknown>;
  add?: { label: string; selected: boolean; run: () => void };
}) {
  const keys = () => props.tabs.map((tab) => tab.key);
  const { shown, arrange } = createArrangement(keys);
  const lift = (key: string) =>
    liftOnPhone(
      IN_TABS,
      liftTab(key, keys, arrange, (to) => props.move(key, to)),
    );

  return (
    <div class="tabs folder">
      <div
        class="tabrow"
        ref={(row) => {
          slidingThumb(row);
          animateList(".tab")(row);
        }}
      >
        <For each={shown()}>
          {(key) => (
            <Show when={props.tabs.find((tab) => tab.key === key)}>
              {(tab) => (
                <span
                  ref={(element) => {
                    keepInView(() => key === props.selected)(element);
                    pressed(
                      () => props.choose(key),
                      () => {},
                      lift(key),
                    ).ref(element);
                  }}
                  class="tab"
                  classList={{ on: key === props.selected }}
                  data-key={key}
                  role="button"
                >
                  <Show when={tab().loading}>
                    <Glyph status="working" />
                  </Show>
                  <span class="name">{tab().name}</span>
                  <Show when={tab().dirty}>
                    <i class="udot" />
                  </Show>
                  <Show when={tab().hint}>{(hint) => <Id>{hint()}</Id>}</Show>
                  <span
                    class="x"
                    role="button"
                    aria-label={`Close ${tab().name}`}
                    on:click={(event) => {
                      event.stopPropagation();
                      props.close(key);
                    }}
                  >
                    <Icon name="x" />
                  </span>
                </span>
              )}
            </Show>
          )}
        </For>
        <Show when={props.add}>
          {(add) => (
            <span
              ref={keepInView(() => add().selected)}
              class="tab plus"
              classList={{ on: add().selected }}
              role="button"
              aria-label={add().label}
              onClick={() => add().run()}
            >
              <Icon name="plus" />
            </span>
          )}
        </Show>
      </div>
    </div>
  );
}

/** A screen's top bar over a crumb in its section's color, back to the screen named `up`. */
export function CrumbBar(props: {
  up: string;
  color: string;
  crumb: JSX.Element;
  title: JSX.Element;
  end?: JSX.Element;
  slim?: boolean;
}) {
  return (
    <div class="bar" classList={{ slim: props.slim }}>
      <Circle icon="back" label={props.up} tap={back} />
      <div class="ttl" style={{ "--ws": props.color }}>
        <span class="crumb">
          <i />
          {props.crumb}
        </span>
        <b class="name">{props.title}</b>
      </div>
      {props.end}
    </div>
  );
}

/** A screen's 56pt top bar: back when the screen has one, then its title or crumbs over the title, then actions. */
export function Bar(props: { back?: () => void; children: JSX.Element; end?: JSX.Element }) {
  return (
    <div class="bar">
      <Show when={props.back}>{(back) => <Circle icon="back" label="Back" tap={back()} />}</Show>
      {props.children}
      {props.end}
    </div>
  );
}
