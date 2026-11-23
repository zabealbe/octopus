//! The TypeScript the UI compiles against, rendered from the Rust types and constants that cross the IPC boundary.

use octopus_proto::ops::{
    Entry, EntryKind, Folder, HarnessStatus, Inside, Match, Offer, OsSetting, PairedAccount, PairedClient, Subfolder,
};
use octopus_proto::settings::{AgentFlags, Current, Setting};
use octopus_proto::term::HISTORY_LINES;
use octopus_proto::types::{
    AGENT_PANEL, Agent, AgentStatus, BACKGROUND_BROWSER_PANEL, BrowserId, BrowserPanel, BrowserTarget, FRESH_SESSION,
    HOLDER_GRACE_MS, Hue, Listening, LockHolder, Mode, NEW_BROWSER_PANEL, Panel, PtyId, Session, SessionId, Swatch,
    Task, TaskId, TaskState, Tone, WorkspaceId,
};
use serde::Serialize;
use serde_json::json;
use ts_rs::Config;

use crate::browsing::{
    DOWNLOADED_EVENT, Downloaded, FAILED_EVENT, Failed, HELD_EVENT, Held, OPENED_EVENT, OnScreen, Opened, PAGED_EVENT,
    Paged,
};
use crate::client_menu::{Appearance, ZOOMS, Zoom, ZoomStep};
use crate::commands::{Boot, NewTask, ShownLink, ShownOffer};
use crate::hub::{
    ACCOUNT_EVENT, AccountView, DeviceView, FILES_CHANGED_EVENT, FilesChanged, MODEL_EVENT, Model, SERVERS_EVENT,
    ServerView,
};
use crate::microphone::MICROPHONE_DENIED_EVENT;
use crate::task_lists::{Column, Glyph, Lists, Row, Section, TaskRef, WorkspaceRef};
use crate::task_view::{PanelView, TaskView};
use crate::terminal::TermScreen;
use crate::ui::{
    Bounds, FILE_DRAG_EVENT, FOCUSED_EVENT, FileDrag, Floated, Modal, Prefs, Reveal, SHOWN_EVENT, Split, Surface,
    UI_EVENT, Ui, UiAct, View, Viewport,
};
use crate::webkit::Go;

/// Where the rendered file lives, relative to the workspace root.
pub const PATH: &str = "ui/shared/src/generated.ts";

#[macro_export]
macro_rules! declarations {
    ($config:expr, $($ty:ty),* $(,)?) => {
        [$(format!("export {}\n", <$ty as ::ts_rs::TS>::decl($config))),*]
    };
}

pub fn constant(name: &str, value: &impl Serialize) -> String {
    format!("export const {name} = {} as const;\n", serde_json::to_string(value).expect("constants serialize"))
}

pub fn render() -> String {
    let config = Config::default().with_large_int("number");
    let declarations = declarations!(
        &config,
        TaskId,
        WorkspaceId,
        PtyId,
        BrowserId,
        BrowserTarget,
        SessionId,
        Session,
        TaskState,
        Mode,
        AgentStatus,
        Hue,
        Tone,
        Swatch,
        Agent,
        Panel,
        Listening,
        LockHolder,
        BrowserPanel,
        Task,
        EntryKind,
        Entry,
        Match,
        Subfolder,
        OsSetting,
        Inside,
        Folder,
        HarnessStatus,
        Offer,
        ShownOffer,
        PairedClient,
        PairedAccount,
        ShownLink,
        AccountView,
        DeviceView,
        TaskRef,
        Glyph,
        Row,
        Column,
        Section,
        WorkspaceRef,
        Lists,
        PanelView,
        TaskView,
        TermScreen,
        ServerView,
        Model,
        FilesChanged,
        View,
        Surface,
        Split,
        Modal,
        Appearance,
        Zoom,
        ZoomStep,
        AgentFlags,
        Setting,
        Current,
        Prefs,
        Reveal,
        Ui,
        UiAct,
        Viewport,
        Boot,
        NewTask,
        Bounds,
        Floated,
        FileDrag,
        OnScreen,
        Go,
        Paged,
        Failed,
        Opened,
        Held,
        Downloaded,
    );
    let constants = [
        constant("TASK_STATES", &TaskState::ALL),
        constant("LIVE_STATES", &TaskState::LIVE),
        constant("MODES", &Mode::ALL),
        constant("HUES", &Hue::ALL),
        constant("TONES", &Tone::ALL),
        constant("ZOOMS", &ZOOMS),
        constant("AGENT_PANEL", &AGENT_PANEL),
        constant("NEW_BROWSER_PANEL", &NEW_BROWSER_PANEL),
        constant("BACKGROUND_BROWSER_PANEL", &BACKGROUND_BROWSER_PANEL),
        constant("FRESH_SESSION", &FRESH_SESSION),
        constant("TERM_HISTORY_LINES", &HISTORY_LINES),
        constant("HOLDER_GRACE_MS", &HOLDER_GRACE_MS),
        constant("COMMANDS", &crate::COMMANDS),
        constant(
            "EVENTS",
            &json!({
                "model": MODEL_EVENT,
                "servers": SERVERS_EVENT,
                "account": ACCOUNT_EVENT,
                "filesChanged": FILES_CHANGED_EVENT,
                "ui": UI_EVENT,
                "windowShown": SHOWN_EVENT,
                "windowFocused": FOCUSED_EVENT,
                "fileDrag": FILE_DRAG_EVENT,
                "microphoneDenied": MICROPHONE_DENIED_EVENT,
                "browserPaged": PAGED_EVENT,
                "browserFailed": FAILED_EVENT,
                "browserOpened": OPENED_EVENT,
                "browserHeld": HELD_EVENT,
                "browserDownloaded": DOWNLOADED_EVENT,
            }),
        ),
    ];
    let header =
        "// Rendered from the Rust types by `cargo run -p octopus-desktop --example types`. Edit those, not this.\n";
    std::iter::once(header.to_string()).chain(declarations).chain(constants).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_ui_compiles_against_the_current_rust_types() {
        let committed = include_str!("../../../ui/shared/src/generated.ts");
        assert!(
            committed == super::render(),
            "{} drifted from the Rust types; run `cargo run -p octopus-desktop --example types`",
            super::PATH
        );
    }
}
