mod common;

use std::path::Path;

use common::{
    OCTOPUS, SERVER, ServerProcess, agent, connect, create, echo_agent, eventually, focus, harness, holder_name,
    read_until, refused, reporting_agent, send, server, set_harness, start, status_report, task, task_create,
    workspace,
};
use octopus_proto::Error;
use octopus_proto::ops::{
    AgentReportSession, EnvVar, HarnessList, HarnessSet, TaskCreate, TaskFocus, TaskMove, TaskSetSession, TermClaim,
    TermClose, TermOpen, TermResize, TermSend, TermTakeLock,
};
use octopus_proto::types::{
    Agent, AgentStatus, ENV_SERVER, ENV_TASK, FRESH_SESSION, Mode, SessionId, SessionStart, Stream, TaskId, TaskState,
};
use octopus_server::{AGENT_RECLAIM_MS, now};

fn locked(error: Error, holder: &str) {
    assert_eq!(error, Error::PtyLocked { holder: holder.into() });
}

#[tokio::test(flavor = "multi_thread")]
async fn a_focused_task_runs_its_agent_terminal_end_to_end() {
    let (dir, running) = server().await;
    let laptop = connect(&running.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    let blocked = reporting_agent("blocked", &format!("agent:${ENV_TASK}@${ENV_SERVER}"));
    set_harness(&laptop, "echo", blocked).await;

    let id = create(&laptop, &workspace, Mode::InPlace, "echo").await;
    assert_eq!(task(&laptop, &id).await.agent, Agent::NotStarted);
    focus(&laptop, &id).await;
    let waiting = Agent::Running { status: AgentStatus::Waiting };
    eventually("the agent reporting it is blocked", async || task(&laptop, &id).await.agent == waiting).await;
    let agent = agent(&laptop, &id).await;

    let mut live = laptop.open_data_stream(&agent).await.expect("attach");
    read_until(&mut live, &format!("agent:{id}@{SERVER}")).await;
    let claim = |cols, rows| TermClaim { task: id.clone(), panel: "agent".into(), cols, rows };
    laptop.call(&claim(80, 24)).await.expect("claim the free terminal");
    let write = TermSend { take_lock: false, ..send(&id, "agent", "ping") };
    laptop.call(&write).await.expect("term.send");
    read_until(&mut live, "ping").await;
    assert_eq!(holder_name(&task(&laptop, &id).await.panels[0]), Some("laptop"));

    let desk = connect(&running.socket, "desk").await;
    let mut replay = desk.open_data_stream(&agent).await.expect("reattach");
    read_until(&mut replay, "ping").await;
    locked(refused(desk.call(&claim(90, 20)).await), "laptop");
    locked(refused(desk.call(&write).await), "laptop");
    let resize = TermResize { task: id.clone(), panel: "agent".into(), cols: 90, rows: 20 };
    locked(refused(desk.call(&resize).await), "laptop");
    let Stream::Pty(pty) = &agent else { unreachable!("an agent terminal is a PTY") };
    locked(refused(desk.open_data_stream(&Stream::Mic(pty.clone())).await.map(drop)), "laptop");
    desk.call(&TermTakeLock { task: id.clone(), panel: "agent".into(), cols: 100, rows: 30 }).await.expect("take");
    let panel = task(&laptop, &id).await.panels.remove(0);
    assert_eq!((holder_name(&panel), panel.cols, panel.rows), (Some("desk"), 100, 30));
    drop(replay);
    eventually("a holder that stops showing the terminal freeing its lock", async || {
        task(&laptop, &id).await.panels[0].lock_holder.is_none()
    })
    .await;

    laptop.call(&TaskMove { task: id.clone(), state: TaskState::Completed }).await.expect("task.move");
    eventually("the agent exiting after its task completed", async || !task(&laptop, &id).await.panels[0].running)
        .await;
    assert_eq!(task(&laptop, &id).await.agent, Agent::NotStarted);
    read_until(&mut desk.open_data_stream(&agent).await.expect("attach exited"), "ping").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn text_with_enter_arrives_as_a_bracketed_paste_then_enter() {
    let (dir, running) = server().await;
    let laptop = connect(&running.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    let pasting = vec!["sh".into(), "-c".into(), r"printf '\033[?2004h'; echo ready; exec cat -v".into()];
    set_harness(&laptop, "pasting", pasting).await;
    let id = create(&laptop, &workspace, Mode::InPlace, "pasting").await;
    focus(&laptop, &id).await;
    let mut live = laptop.open_data_stream(&agent(&laptop, &id).await).await.expect("attach");
    read_until(&mut live, "ready").await;

    laptop.call(&send(&id, "agent", "submit me")).await.expect("term.send");
    read_until(&mut live, "^[[200~submit me^[[201~\r\n").await;
    laptop.call(&send(&id, "agent", "stays\u{1b}[2\u{1b}[201~01~ inside")).await.expect("term.send");
    read_until(&mut live, "^[[200~stays[2[201~01~ inside^[[201~\r\n").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shell_panel_follows_the_agent_until_its_shell_exits_or_it_closes() {
    let (dir, running) = server().await;
    let laptop = connect(&running.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    set_harness(&laptop, "echo", echo_agent("agent:up")).await;
    let id = create(&laptop, &workspace, Mode::InPlace, "echo").await;
    focus(&laptop, &id).await;

    let shell = laptop.call(&TermOpen { task: id.clone(), path: ".".into() }).await.expect("term.open").panel;
    let panels = task(&laptop, &id).await.panels;
    assert_eq!(panels.iter().map(|panel| &panel.id).collect::<Vec<_>>()[1..], [&shell]);
    assert_eq!(panels[0].name, "agent");
    let mut live = laptop.open_data_stream(&Stream::Pty(shell.clone())).await.expect("attach the shell");
    laptop.call(&send(&id, &shell.0, &format!("echo shell:${ENV_TASK}"))).await.expect("type into the shell");
    read_until(&mut live, &format!("shell:{id}")).await;
    let agent_closed = laptop.call(&TermClose { task: id.clone(), panel: "agent".into() }).await;
    assert!(matches!(refused(agent_closed), Error::Conflict { .. }));

    let closed = laptop.call(&TermOpen { task: id.clone(), path: ".".into() }).await.expect("term.open").panel;
    let names: Vec<_> = task(&laptop, &id).await.panels.into_iter().map(|panel| panel.name).collect();
    assert_eq!(names[2], format!("{}-2", names[1]));
    laptop.call(&send(&id, &shell.0, "exit")).await.expect("exit the shell");
    eventually("the exited shell leaving the list", async || task(&laptop, &id).await.panels.len() == 2).await;
    laptop.call(&TermClose { task: id.clone(), panel: closed.0 }).await.expect("term.close");
    assert_eq!(task(&laptop, &id).await.panels.len(), 1);

    laptop.call(&TaskMove { task: id.clone(), state: TaskState::Completed }).await.expect("complete");
    let ended = laptop.call(&TermOpen { task: id, path: ".".into() }).await;
    assert!(matches!(refused(ended), Error::Conflict { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_harness_names_the_harnesses_folder_in_its_args_and_env_and_lists_beside_the_built_in_ones() {
    let (dir, running) = server().await;
    let laptop = connect(&running.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    let env = vec![EnvVar { name: "WHERE".into(), value: "{files}/x".into() }];
    let set = HarnessSet { env, ..harness("echo", echo_agent("files:{files} where:$WHERE")) };
    laptop.call(&set).await.expect("harness.set");
    let listed = laptop.call(&HarnessList {}).await.expect("harness.list").harnesses;
    assert_eq!(listed, ["claude-code", "codex", "echo", "opencode"]);

    let id = create(&laptop, &workspace, Mode::InPlace, "echo").await;
    focus(&laptop, &id).await;
    let mut live = laptop.open_data_stream(&agent(&laptop, &id).await).await.expect("attach");
    let files = dir.path().join("harnesses");
    read_until(&mut live, &format!("files:{0} where:{0}/x", files.display())).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_harness_runs_nothing_and_reports_its_install_command() {
    let (dir, running) = server().await;
    let laptop = connect(&running.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    let install = "npm install -g @anthropic-ai/claude-code".to_string();
    let absent = harness("claude-code", vec!["octopus-test-no-such-binary".into()]);
    laptop.call(&HarnessSet { install: install.clone(), ..absent }).await.expect("harness.set");

    let missing = create(&laptop, &workspace, Mode::InPlace, "claude-code").await;
    focus(&laptop, &missing).await;
    let view = task(&laptop, &missing).await;
    assert_eq!(view.agent, Agent::HarnessNotInstalled { install });
    assert!(view.panels.is_empty());

    let unknown = create(&laptop, &workspace, Mode::InPlace, "aider").await;
    focus(&laptop, &unknown).await;
    assert_eq!(task(&laptop, &unknown).await.agent, Agent::HarnessUnknown);
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_focuses_start_one_agent() {
    let (dir, running) = server().await;
    let starts = dir.path().join("starts");
    let laptop = connect(&running.socket, "laptop").await;
    let desk = connect(&running.socket, "desk").await;
    let workspace = workspace(&laptop, dir.path()).await;
    set_harness(&laptop, "echo", echo_agent(&format!("start >> {}", starts.display()))).await;
    let id = create(&laptop, &workspace, Mode::InPlace, "echo").await;

    let focus = TaskFocus { task: id.clone(), args: vec![] };
    let (first, second) = tokio::join!(laptop.call(&focus), desk.call(&focus));
    first.expect("focus from laptop");
    second.expect("focus from desk");
    eventually("the agent writing its start line", async || starts.exists()).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(std::fs::read_to_string(&starts).expect("read starts").lines().count(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_outlives_a_restart_of_its_server_process() {
    let dir = tempfile::tempdir().expect("tempdir");
    let before = ServerProcess::run(dir.path()).await;
    let laptop = connect(&before.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    set_harness(&laptop, "echo", echo_agent("agent:up")).await;
    let id = create(&laptop, &workspace, Mode::InPlace, "echo").await;
    focus(&laptop, &id).await;
    laptop.call(&TermOpen { task: id.clone(), path: ".".into() }).await.expect("term.open");
    let view = task(&laptop, &id).await;
    let agent = Stream::Pty(view.panels[0].id.clone());
    let mut live = laptop.open_data_stream(&agent).await.expect("attach");
    read_until(&mut live, "agent:up").await;
    laptop.call(&send(&id, "agent", "before-restart")).await.expect("term.send");
    read_until(&mut live, "before-restart").await;
    before.kill();

    let after = ServerProcess::run(dir.path()).await;
    let laptop = connect(&after.socket, "laptop").await;
    let reattached = task(&laptop, &id).await;
    let ids = |panels: &[octopus_proto::types::Panel]| panels.iter().map(|panel| panel.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&reattached.panels), ids(&view.panels));
    assert_eq!(reattached.agent, Agent::Running { status: AgentStatus::Idle });
    let mut replay = laptop.open_data_stream(&agent).await.expect("reattach");
    read_until(&mut replay, "before-restart").await;
    laptop.call(&send(&id, "agent", "after-restart")).await.expect("term.send after the restart");
    read_until(&mut replay, "after-restart").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_machine_restart_the_next_focus_starts_the_agent_again() {
    let dir = tempfile::tempdir().expect("tempdir");
    let before = start(dir.path(), "before.sock", "host.sock").await;
    let laptop = connect(&before.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    set_harness(&laptop, "echo", echo_agent("agent:up")).await;
    let id = create(&laptop, &workspace, Mode::InPlace, "echo").await;
    focus(&laptop, &id).await;

    let after = start(dir.path(), "after.sock", "rebooted-host.sock").await;
    let laptop = connect(&after.socket, "laptop").await;
    let forgotten = task(&laptop, &id).await;
    assert_eq!((forgotten.agent, forgotten.panels), (Agent::NotStarted, vec![]));
    focus(&laptop, &id).await;
    assert_eq!(task(&laptop, &id).await.agent, Agent::Running { status: AgentStatus::Idle });
}

#[allow(unsafe_code, reason = "libSystem's responsibility call has no safe binding")]
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_owns_its_terminal_and_on_macos_answers_to_the_terminal_host_for_privacy_permissions() {
    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        fn responsibility_get_pid_responsible_for_pid(pid: i32) -> i32;
    }
    // Perl rather than a shell: bash takes its terminal as its controlling terminal on its own, which would hide a
    // spawn that leaves the PTY nobody's. tcgetpgrp answers only on the caller's controlling terminal.
    const PROBE: &str = r#"use POSIX; $| = 1;
        $SIG{WINCH} = sub { print "resized\n" };
        $SIG{INT} = sub { print "interrupted\n" };
        print "probe:$$:", (tcgetpgrp(0) == $$ ? 'foreground' : 'not foreground'), ":", getppid(), ":end\n";
        sleep 1 while 1;"#;
    let (dir, running) = server().await;
    let laptop = connect(&running.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    set_harness(&laptop, "probe", vec!["perl".into(), "-e".into(), PROBE.into()]).await;
    let id = create(&laptop, &workspace, Mode::InPlace, "probe").await;
    focus(&laptop, &id).await;
    let mut live = laptop.open_data_stream(&agent(&laptop, &id).await).await.expect("attach");
    let seen = read_until(&mut live, ":end").await;
    let line = seen.split("probe:").nth(1).and_then(|rest| rest.split(":end").next()).expect("the probe's line");
    let pid: i32 = line.split(':').next().and_then(|pid| pid.parse().ok()).expect("a pid");
    let host: i32 = line.rsplit(':').next().and_then(|host| host.parse().ok()).expect("the host's pid");
    assert_eq!(line, format!("{pid}:foreground:{host}"), "the agent's controlling terminal is its PTY, which it leads");

    laptop.call(&interrupt(&id)).await.expect("ctrl-c");
    read_until(&mut live, "interrupted").await;
    laptop.call(&TermResize { task: id, panel: "agent".into(), cols: 120, rows: 40 }).await.expect("resize");
    read_until(&mut live, "resized").await;
    // SAFETY: the call reads only the kernel's record of a pid.
    #[cfg(target_os = "macos")]
    assert_eq!(unsafe { responsibility_get_pid_responsible_for_pid(pid) }, host);
}

/// A harness whose agents append the args after their argv to `starts`, one line per start, starting and resuming
/// their session by id.
fn sessions_harness(id: &str, starts: &Path) -> HarnessSet {
    let resume = ["--resume", "{session}"].map(String::from).to_vec();
    let script = format!("printf '%s\\n' \"$*\" >> '{}'; exec cat", starts.display());
    let argv = vec!["sh".into(), "-c".into(), script, "sh".into()];
    HarnessSet { resume, ..harness(id, argv) }
}

/// Waits for one start per line, each with args ending in that line.
async fn started_with(starts: &Path, lines: &[String]) {
    eventually(&format!("the agents starting with {lines:?}"), async || {
        let read = std::fs::read_to_string(starts).unwrap_or_default();
        read.lines().count() == lines.len() && read.lines().zip(lines).all(|(read, line)| read.ends_with(line.as_str()))
    })
    .await;
}

fn interrupt(id: &TaskId) -> TermSend {
    TermSend { enter: false, ..send(id, "agent", "\u{3}") }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_start_resumes_the_session_its_agent_last_reported_with_the_args_of_its_focus() {
    let (dir, running) = server().await;
    let starts = dir.path().join("starts");
    let laptop = connect(&running.socket, "laptop").await;
    let desk = connect(&running.socket, "desk").await;
    let workspace = workspace(&laptop, dir.path()).await;
    laptop.call(&sessions_harness("sessions", &starts)).await.expect("harness.set");
    let opus = async |id: &TaskId| {
        let args = ["--model", "opus"].map(String::from).to_vec();
        laptop.call(&TaskFocus { task: id.clone(), args }).await.expect("task.focus");
    };
    let report = async |id: &TaskId, session: &str| {
        let report = AgentReportSession { task: id.clone(), session: SessionId(session.into()) };
        laptop.call(&report).await.expect("agent.report_session")
    };

    let id = create(&laptop, &workspace, Mode::InPlace, "sessions").await;
    assert_eq!(task(&laptop, &id).await.session, None);
    opus(&id).await;
    let mut lines = vec!["--model opus".to_string()];
    started_with(&starts, &lines).await;
    let running_panels = task(&laptop, &id).await.panels;
    let reported = report(&id, "first").await.rev;
    assert_eq!(report(&id, "first").await.rev, reported, "the session the task holds changes nothing");
    report(&id, "cleared").await;
    assert_eq!(task(&laptop, &id).await.session, Some(SessionId("cleared".into())));
    desk.call(&TaskFocus { task: id.clone(), args: vec!["--other".into()] }).await.expect("focus the running agent");
    assert_eq!(task(&desk, &id).await.panels, running_panels, "a report and a focus leave the agent running");

    laptop.call(&interrupt(&id)).await.expect("ctrl-c");
    eventually("the interrupted agent ending", async || task(&laptop, &id).await.agent == Agent::NotStarted).await;
    assert!(!task(&laptop, &id).await.panels[0].running, "the ended agent's panel stays, showing its scrollback");
    desk.call(&TaskFocus { task: id.clone(), args: vec!["--other".into()] }).await.expect("task.focus");
    lines.push("--other --resume cleared".into());
    started_with(&starts, &lines).await;

    laptop.call(&TaskMove { task: id.clone(), state: TaskState::Completed }).await.expect("complete");
    laptop.call(&TaskMove { task: id.clone(), state: TaskState::Pinned }).await.expect("reopen");
    assert_eq!(task(&laptop, &id).await.panels, vec![], "a reopened task's exited agent goes");
    opus(&id).await;
    lines.push("--model opus --resume cleared".into());
    started_with(&starts, &lines).await;

    for session in ["", FRESH_SESSION] {
        let set = TaskSetSession { task: id.clone(), session: SessionId(session.into()) };
        assert!(matches!(refused(laptop.call(&set).await), Error::InvalidArgs { .. }), "{session:?}");
    }
    let empty = AgentReportSession { task: id.clone(), session: SessionId(String::new()) };
    assert!(matches!(refused(laptop.call(&empty).await), Error::InvalidArgs { .. }));
    let set = TaskSetSession { task: id.clone(), session: SessionId("elsewhere".into()) };
    laptop.call(&set).await.expect("task.set_session");
    assert_eq!(task(&laptop, &id).await.panels, vec![]);
    opus(&id).await;
    lines.push("--model opus --resume elsewhere".into());
    started_with(&starts, &lines).await;

    let imported = SessionStart::Resume(SessionId("demo-session".into()));
    let imported = TaskCreate { session: imported, ..task_create(&workspace, dir.path(), Mode::InPlace, "sessions") };
    let imported = laptop.call(&imported).await.expect("task.create").task;
    opus(&imported).await;
    lines.push("--model opus --resume demo-session".into());
    started_with(&starts, &lines).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_that_cannot_start_fails_the_focus_with_the_reason() {
    use std::os::unix::fs::PermissionsExt;

    let (dir, running) = server().await;
    let script = dir.path().join("script");
    std::fs::write(&script, "#!/no/such/interpreter\n").expect("write");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let laptop = connect(&running.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    set_harness(&laptop, "script", vec![script.to_string_lossy().into()]).await;
    let id = create(&laptop, &workspace, Mode::InPlace, "script").await;
    let message = format!("start {}: No such file or directory (os error 2)", script.display());
    assert_eq!(refused(laptop.call(&TaskFocus { task: id, args: vec![] }).await), Error::Internal { message });
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hook_reports_its_session_through_the_report_session_script_and_never_fails_the_agent() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let (dir, running) = server().await;
    let laptop = connect(&running.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    let id = create(&laptop, &workspace, Mode::InPlace, "claude-code").await;
    let home = dir.path().join("home");
    let added = Command::new(OCTOPUS)
        .env("HOME", &home)
        .args(["server", "add", "--name", SERVER, "--socket"])
        .arg(&running.socket)
        .output()
        .expect("server add");
    assert!(added.status.success(), "{added:?}");
    let hook = |input: &str| {
        let mut child = Command::new(dir.path().join("harnesses/report-session"))
            .env("HOME", &home)
            .env(ENV_SERVER, SERVER)
            .env(ENV_TASK, &id.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run report-session");
        child.stdin.take().expect("stdin").write_all(input.as_bytes()).expect("write the hook's JSON");
        let output = child.wait_with_output().expect("report-session ends");
        assert!(output.status.success() && output.stdout.is_empty(), "{output:?}");
        String::from_utf8_lossy(&output.stderr).into_owned()
    };

    assert_eq!(hook(r#"{"session_id":"ses_1","hook_event_name":"UserPromptSubmit"}"#), "");
    assert_eq!(task(&laptop, &id).await.session, Some(SessionId("ses_1".into())));
    assert!(hook("no hook's JSON").contains("octopus agent hook"));
    assert_eq!(task(&laptop, &id).await.session, Some(SessionId("ses_1".into())));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launched_server_finds_and_runs_agents_in_the_login_environment() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    let agent_path = bin.join("octopus-test-agent");
    std::fs::write(&agent_path, "#!/bin/sh\necho \"login:$OCTOPUS_TEST_LOGIN\"\nexec cat\n").expect("agent");
    std::fs::set_permissions(&agent_path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let server = ServerProcess::launched(dir.path(), &home).await;
    let laptop = connect(&server.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    set_harness(&laptop, "login", vec!["octopus-test-agent".into()]).await;
    let id = create(&laptop, &workspace, Mode::InPlace, "login").await;

    focus(&laptop, &id).await;
    assert_eq!(task(&laptop, &id).await.agent, Agent::HarnessNotInstalled { install: "true".into() });

    let profile = format!("export PATH=\"{}:$PATH\"\nexport OCTOPUS_TEST_LOGIN=profile\n", bin.display());
    for file in [".profile", ".zprofile"] {
        std::fs::write(home.join(file), &profile).expect("profile");
    }
    focus(&laptop, &id).await;
    read_until(&mut laptop.open_data_stream(&agent(&laptop, &id).await).await.expect("attach"), "login:profile").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_server_stops_an_agent_idle_silent_and_unshown_for_the_reclaim_time() {
    let (dir, running) = server().await;
    let laptop = connect(&running.socket, "laptop").await;
    let workspace = workspace(&laptop, dir.path()).await;
    let sh = |script: &str| ["sh", "-c", script].map(String::from).to_vec();
    set_harness(&laptop, "agent", sh("sleep 600 & exec cat")).await;
    set_harness(&laptop, "detaching", sh("set -m; sleep 60 & wait")).await;
    let before = now();
    let mut tasks = Vec::new();
    for harness in ["agent", "agent", "agent", "detaching"] {
        let id = create(&laptop, &workspace, Mode::InPlace, harness).await;
        focus(&laptop, &id).await;
        tasks.push(id);
    }
    let [idle, waiting, shown, detached] = <[TaskId; 4]>::try_from(tasks).expect("four tasks");
    laptop.call(&send(&waiting, "agent", &status_report("blocked"))).await.expect("term.send");
    let blocked = Agent::Running { status: AgentStatus::Waiting };
    eventually("the agent reporting it is blocked", async || task(&laptop, &waiting).await.agent == blocked).await;
    let _showing = laptop.open_data_stream(&agent(&laptop, &shown).await).await.expect("show");
    let after = now();

    assert_eq!(running.server.reclaim_agents(before + AGENT_RECLAIM_MS - 1), vec![]);
    let due = running.server.reclaim_agents(after + AGENT_RECLAIM_MS);
    assert_eq!(due, vec![idle.clone()], "a waiting, a shown and a detaching agent keep running");
    let asleep = task(&laptop, &idle).await;
    assert_eq!((asleep.agent, asleep.panels), (Agent::NotStarted, vec![]), "a reclaimed agent leaves its panels");
    for kept in [&waiting, &shown, &detached] {
        assert!(matches!(task(&laptop, kept).await.agent, Agent::Running { .. }));
    }
    focus(&laptop, &idle).await;
    assert_eq!(task(&laptop, &idle).await.agent, Agent::Running { status: AgentStatus::Idle });
}
