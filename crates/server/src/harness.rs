//! The harnesses every server knows, and the files in the server's harnesses folder they read.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use octopus_proto::ops::EnvVar;

use crate::store::Harness;
use crate::terminals::shell_word;

/// Stands for the task's session id in a harness's resume args.
pub(crate) const SESSION: &str = "{session}";
/// Stands for the server's harnesses folder in a harness's args and env values.
pub(crate) const FILES: &str = "{files}";

const CLAUDE_CODE: &str = "claude-code";
const CODEX: &str = "codex";
const OPENCODE: &str = "opencode";

/// Every harness the server knows by its id until a `harness.set` stores one in its place.
pub(crate) fn built_ins() -> Vec<(&'static str, Harness)> {
    let strings = |items: &[&str]| items.iter().map(ToString::to_string).collect();
    // Codex runs `report-session` at the first turn of each session, once its user trusts the hook.
    let codex_hook =
        format!(r#"hooks.SessionStart=[{{hooks=[{{type="command",command="{FILES}/{REPORT_SESSION}"}}]}}]"#);
    vec![
        (
            CLAUDE_CODE,
            Harness {
                argv: strings(&["claude", "--plugin-dir", &format!("{FILES}/{CLAUDE_CODE}")]),
                resume: strings(&["--resume", SESSION]),
                env: Vec::new(),
                install: "npm install -g @anthropic-ai/claude-code".into(),
            },
        ),
        (
            CODEX,
            Harness {
                argv: strings(&["codex", "--no-daemon", "-c", &codex_hook]),
                resume: strings(&["resume", SESSION]),
                env: Vec::new(),
                install: "npm install -g @openai/codex".into(),
            },
        ),
        (
            OPENCODE,
            Harness {
                argv: strings(&["opencode", "--standalone"]),
                resume: strings(&["--session", SESSION]),
                env: vec![EnvVar {
                    name: "OPENCODE_CONFIG_CONTENT".into(),
                    value: format!(r#"{{"plugins":["{FILES}/{OPENCODE}"]}}"#),
                }],
                install: "npm install -g @opencode/cli".into(),
            },
        ),
    ]
}

pub(crate) fn built_in(id: &str) -> Option<Harness> {
    built_ins().into_iter().find(|(known, _)| *known == id).map(|(_, harness)| harness)
}

/// The script each built-in harness's hook runs, handing the hook's JSON to `octopus agent hook`. It exits 0 whatever
/// the binary does, since an agent reads another exit as the hook blocking the prompt.
const REPORT_SESSION: &str = "report-session";

/// Lays out in `files` what the built-in harnesses read: `skill`, the octopus skill's SKILL.md, and the
/// `report-session` script running `octopus`, this server's own binary.
pub fn lay_out_files(files: &Path, skill: &str, octopus: &Path) -> std::io::Result<()> {
    let report_session = files.join(REPORT_SESSION);
    let script = format!(
        "#!/bin/sh\n{} agent hook --server \"$OCTOPUS_SERVER\" --task \"$OCTOPUS_TASK\"\nexit 0\n",
        shell_word(&octopus.to_string_lossy())
    );
    write(&report_session, &script)?;
    std::fs::set_permissions(&report_session, std::fs::Permissions::from_mode(0o755))?;

    let claude_code = files.join(CLAUDE_CODE);
    let hook = serde_json::json!({ "type": "command", "command": shell_word(&report_session.to_string_lossy()) });
    let hooks = serde_json::json!({ "hooks": { "UserPromptSubmit": [{ "hooks": [hook] }] } });
    write(&claude_code.join(".claude-plugin/plugin.json"), CLAUDE_CODE_PLUGIN)?;
    write(&claude_code.join("hooks/hooks.json"), &hooks.to_string())?;
    write(&claude_code.join("skills/octopus/SKILL.md"), skill)?;

    let path = serde_json::to_string(&report_session).expect("a path is JSON");
    write(&files.join(OPENCODE).join("index.js"), &OPENCODE_PLUGIN.replace(REPORT_SESSION_PATH, &path))
}

const CLAUDE_CODE_PLUGIN: &str = r#"{ "name": "octopus", "description": "The octopus skill and session reports, for agents octopus runs." }
"#;

const REPORT_SESSION_PATH: &str = "REPORT_SESSION_PATH";

/// Reports each session OpenCode creates without a parent, as its subagents' have one. A report that fails is lost
/// alone, rather than ending the reports after it or OpenCode's server, which an unheard error on a child would.
const OPENCODE_PLUGIN: &str = r#"import { spawn } from "node:child_process"

const report = (sessionID) => {
  try {
    const hook = spawn(REPORT_SESSION_PATH, { stdio: ["pipe", "ignore", "ignore"] })
    hook.on("error", () => {})
    hook.stdin?.on("error", () => {})
    hook.stdin?.end(JSON.stringify({ session_id: sessionID }))
  } catch {}
}

export default {
  id: "octopus",
  async setup(ctx) {
    const controller = new AbortController()
    const reports = async () => {
      for await (const event of ctx.event.subscribe({ types: ["session.created"], signal: controller.signal })) {
        if (!event.data.parentID) report(event.data.sessionID)
      }
    }
    reports().catch(() => {})
    return () => controller.abort()
  },
}
"#;

/// Swaps in a whole new file, keeping the old one's permissions, and leaves one that already holds `text` untouched,
/// so an agent's hook never reads one half written.
fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|held| held == text) {
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().expect("a harness file sits in a folder"))?;
    let name = path.file_name().expect("a harness file has a name").to_string_lossy();
    let fresh = path.with_file_name(format!("{name}.writing"));
    std::fs::write(&fresh, text)?;
    if let Ok(held) = std::fs::metadata(path) {
        std::fs::set_permissions(&fresh, held.permissions())?;
    }
    std::fs::rename(&fresh, path)
}
