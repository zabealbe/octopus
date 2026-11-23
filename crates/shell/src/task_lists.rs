use std::collections::{BTreeMap, HashMap};

use octopus_client::cache::Cache;
use octopus_proto::types::{
    AGENT_PANEL, Agent, AgentStatus, Color, Hue, Swatch, Task, TaskId, TaskState, Tone, WorkspaceId,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::saturate;

/// A task anywhere in the client: task ids are unique only within their server.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskRef {
    pub server: String,
    pub task: TaskId,
}

impl TaskRef {
    pub fn key(&self) -> String {
        format!("{}/{}", self.server, self.task)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Glyph {
    Working,
    Waiting,
    Idle,
    /// The agent lists no panel, as the server reclaimed it or it has not run since the task went live or the
    /// terminal host started: showing the task wakes it.
    Asleep,
    /// The agent's panel ended, which only Resume starts again, or its harness is missing, which each focus checks again.
    Ended,
    /// Asleep or ended while a focus this client sent to start it is on its way.
    Waking,
    Completed,
    Cancelled,
}

impl Glyph {
    pub fn of(task: &Task) -> Self {
        match (task.state, &task.agent) {
            (TaskState::Completed, _) => Self::Completed,
            (TaskState::Cancelled, _) => Self::Cancelled,
            (_, Agent::Running { status: AgentStatus::Working }) => Self::Working,
            (_, Agent::Running { status: AgentStatus::Waiting }) => Self::Waiting,
            (_, Agent::Running { status: AgentStatus::Idle }) => Self::Idle,
            _ if awaits_resume(task) => Self::Ended,
            (_, Agent::NotStarted) => Self::Asleep,
            (_, Agent::HarnessNotInstalled { .. } | Agent::HarnessUnknown) => Self::Ended,
        }
    }

    /// The glyph this client shows for the task at `at`, waking while its focus is on its way.
    pub(crate) fn shown(at: &TaskRef, task: &Task, waking: &Waking) -> Self {
        match Self::of(task) {
            Self::Asleep | Self::Ended if waking.contains_key(at) => Self::Waking,
            glyph => glyph,
        }
    }
}

/// Whether the task's agent panel ended, so its agent starts again only through Resume.
pub(crate) fn awaits_resume(task: &Task) -> bool {
    task.panels.iter().any(|panel| panel.name == AGENT_PANEL && !panel.running)
}

/// The tasks this client is focusing: `None` while a focus is on its way, then the rev the server answered with, until
/// the client's cache of the server reaches it.
pub(crate) type Waking = BTreeMap<TaskRef, Option<u64>>;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Row {
    pub key: String,
    pub at: TaskRef,
    pub title: String,
    pub depth: u32,
    /// The task's own state, which a subtask's group or column may differ from.
    pub state: TaskState,
    pub glyph: Glyph,
}

/// One state's tasks in one section: a sidebar group or a board column.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Column {
    pub state: TaskState,
    pub rows: Vec<Row>,
}

/// Every server's workspaces that share one name.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Section {
    pub name: String,
    /// The section's color, with auto resolved.
    pub swatch: Swatch,
    /// The color auto resolves to for the section's name.
    pub auto: Swatch,
    pub live: u32,
    /// Whether the agent of one of its live tasks is waiting on the user.
    pub waiting: bool,
    /// The sidebar's groups, one per live state, of the live tasks.
    pub groups: Vec<Column>,
    /// The board's columns, one per state in `TaskState::ALL` order, of the unarchived tasks.
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct WorkspaceRef {
    pub server: String,
    pub workspace: WorkspaceId,
    pub name: String,
    /// The directory on the server's machine.
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Lists {
    pub sections: Vec<Section>,
    pub workspaces: Vec<WorkspaceRef>,
}

/// The hues auto draws from: none reads as a task's status or as the accent.
const AUTO_HUES: [Hue; 5] = [Hue::Teal, Hue::Sky, Hue::Blue, Hue::Violet, Hue::Pink];

/// The soft tone of a hue hashed from the section's name.
pub(crate) fn auto_swatch(name: &str) -> Swatch {
    let hash = name.chars().fold(7_u32, |hash, char| hash.wrapping_mul(31).wrapping_add(u32::from(char)));
    Swatch { hue: AUTO_HUES[hash as usize % AUTO_HUES.len()], tone: Tone::Soft }
}

/// A picked color beats auto, and among picked colors the first server's by name wins.
fn section_swatch(caches: &BTreeMap<String, Cache>, name: &str) -> Swatch {
    let mut workspaces = caches.values().flat_map(|cache| cache.workspaces.values());
    let picked = workspaces.find_map(|workspace| match workspace.color {
        Color::Picked(swatch) if workspace.name == name => Some(swatch),
        _ => None,
    });
    picked.unwrap_or_else(|| auto_swatch(name))
}

pub(crate) fn section_of<'a>(cache: &'a Cache, task: &'a Task) -> &'a str {
    cache.workspaces.get(&task.workspace).map_or(task.workspace.0.as_str(), |workspace| workspace.name.as_str())
}

struct Placed<'a> {
    at: TaskRef,
    task: &'a Task,
    section: &'a str,
    parent: Option<usize>,
}

impl Placed<'_> {
    fn row(&self, depth: u32, waking: &Waking) -> Row {
        Row {
            key: self.at.key(),
            at: self.at.clone(),
            title: self.task.title.clone(),
            depth,
            state: self.task.state,
            glyph: Glyph::shown(&self.at, self.task, waking),
        }
    }
}

/// One list of every section, holding its unarchived tasks in `states`: a task sits under its parent whenever the list
/// holds the parent in the same section, and heads the column of its own state otherwise.
fn columns<'a>(
    placed: &[Placed<'a>],
    sections: &[&'a str],
    states: &[TaskState],
    waking: &Waking,
) -> BTreeMap<&'a str, Vec<Column>> {
    let column_of =
        |task: &Task| states.iter().position(|state| *state == task.state).filter(|_| task.archived_at.is_none());
    let empty = || vec![Vec::new(); states.len()];
    let mut heads: BTreeMap<&str, Vec<Vec<usize>>> = sections.iter().map(|&name| (name, empty())).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); placed.len()];
    for (i, task) in placed.iter().enumerate() {
        let Some(column) = column_of(task.task) else { continue };
        let parent = task
            .parent
            .filter(|&parent| column_of(placed[parent].task).is_some() && placed[parent].section == task.section);
        match parent {
            Some(parent) => children[parent].push(i),
            None => heads.entry(task.section).or_insert_with(empty)[column].push(i),
        }
    }
    let column = |state: TaskState, heads: Vec<usize>| {
        let mut rows = Vec::new();
        let mut stack: Vec<(usize, u32)> = heads.into_iter().rev().map(|i| (i, 0)).collect();
        while let Some((i, depth)) = stack.pop() {
            rows.push(placed[i].row(depth, waking));
            stack.extend(children[i].iter().rev().map(|&child| (child, depth + 1)));
        }
        Column { state, rows }
    };
    heads
        .into_iter()
        .map(|(name, heads)| (name, states.iter().zip(heads).map(|(&state, heads)| column(state, heads)).collect()))
        .collect()
}

pub fn lists(caches: &BTreeMap<String, Cache>, waking: &Waking) -> Lists {
    let mut placed: Vec<Placed> = caches
        .iter()
        .flat_map(|(server, cache)| {
            cache.tasks.values().map(move |task| Placed {
                at: TaskRef { server: server.clone(), task: task.id.clone() },
                task,
                section: section_of(cache, task),
                parent: None,
            })
        })
        .collect();
    placed.sort_by(|a, b| (a.task.title.to_lowercase(), &a.at).cmp(&(b.task.title.to_lowercase(), &b.at)));
    let index: HashMap<(String, TaskId), usize> =
        placed.iter().enumerate().map(|(i, task)| ((task.at.server.clone(), task.at.task.clone()), i)).collect();
    for task in &mut placed {
        task.parent =
            task.task.parent.as_ref().and_then(|parent| index.get(&(task.at.server.clone(), parent.clone())).copied());
    }

    let names: Vec<&str> =
        caches.values().flat_map(|cache| cache.workspaces.values().map(|workspace| workspace.name.as_str())).collect();
    let mut groups = columns(&placed, &names, &TaskState::LIVE, waking);
    let boards = columns(&placed, &names, &TaskState::ALL, waking);

    let sections = boards
        .into_iter()
        .map(|(name, columns)| {
            let groups = groups.remove(name).unwrap_or_default();
            let rows = || groups.iter().flat_map(|group| &group.rows);
            Section {
                name: name.to_string(),
                swatch: section_swatch(caches, name),
                auto: auto_swatch(name),
                live: saturate(rows().count()),
                waiting: rows().any(|row| row.glyph == Glyph::Waiting),
                columns,
                groups,
            }
        })
        .collect();

    let workspaces = caches
        .iter()
        .flat_map(|(server, cache)| {
            cache.workspaces.values().map(move |workspace| WorkspaceRef {
                server: server.clone(),
                workspace: workspace.id.clone(),
                name: workspace.name.clone(),
                root: workspace.root.clone(),
            })
        })
        .collect();
    Lists { sections, workspaces }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use octopus_proto::types::{Listening, Mode, Panel, PtyId, Workspace};

    #[test]
    fn a_task_shows_its_agent_while_live_and_how_it_ended_after() {
        let mut working = task("tsk_1", "wsp_a", None, "t", TaskState::Temporary);
        working.agent = Agent::Running { status: AgentStatus::Working };
        let mut cancelled = working.clone();
        cancelled.state = TaskState::Cancelled;
        let mut idle = working.clone();
        idle.agent = Agent::Running { status: AgentStatus::Idle };
        let asleep = task("tsk_1", "wsp_a", None, "t", TaskState::Pinned);
        let mut ended = asleep.clone();
        ended.panels = vec![Panel {
            id: PtyId("pty_1".into()),
            name: AGENT_PANEL.into(),
            running: false,
            lock_holder: None,
            cols: 80,
            rows: 24,
            listening: Listening::No,
        }];
        let mut missing = asleep.clone();
        missing.agent = Agent::HarnessUnknown;
        assert_eq!(
            [&working, &idle, &asleep, &ended, &missing, &cancelled].map(Glyph::of),
            [Glyph::Working, Glyph::Idle, Glyph::Asleep, Glyph::Ended, Glyph::Ended, Glyph::Cancelled]
        );
        assert!(awaits_resume(&ended) && !awaits_resume(&asleep), "showing a task wakes all but an ended agent");
    }

    #[test]
    fn a_task_shows_waking_while_this_clients_focus_starts_its_agent() {
        let asleep = task("tsk_1", "wsp_a", None, "t", TaskState::Pinned);
        let at = TaskRef { server: "laptop".into(), task: asleep.id.clone() };
        let waking = Waking::from([(at.clone(), None)]);
        let mut woken = asleep.clone();
        woken.agent = Agent::Running { status: AgentStatus::Idle };
        assert_eq!(
            [Glyph::shown(&at, &asleep, &waking), Glyph::shown(&at, &woken, &waking)],
            [Glyph::Waking, Glyph::Idle]
        );
    }

    pub fn task(id: &str, workspace: &str, parent: Option<&str>, title: &str, state: TaskState) -> Task {
        Task {
            id: TaskId(id.into()),
            workspace: WorkspaceId(workspace.into()),
            parent: parent.map(|parent| TaskId(parent.into())),
            title: title.into(),
            state,
            folder: "/repo".into(),
            mode: Mode::InPlace,
            directory: "/repo".into(),
            branch: None,
            harness: "claude-code".into(),
            session: None,
            ended_at: None,
            archived_at: None,
            rev: 1,
            agent: Agent::NotStarted,
            panels: Vec::new(),
            browsers: Vec::new(),
        }
    }

    pub fn cache(workspaces: &[(&str, &str)], tasks: Vec<Task>) -> Cache {
        Cache {
            rev: 1,
            workspaces: workspaces
                .iter()
                .map(|(id, name)| {
                    let id = WorkspaceId((*id).into());
                    let workspace =
                        Workspace { id: id.clone(), name: (*name).into(), root: "/repo".into(), color: Color::Auto };
                    (id, workspace)
                })
                .collect(),
            tasks: tasks.into_iter().map(|task| (task.id.clone(), task)).collect(),
        }
    }

    fn section<'a>(lists: &'a Lists, name: &str) -> &'a Section {
        lists.sections.iter().find(|candidate| candidate.name == name).expect("section")
    }

    /// Each row as its title, depth, and its own state where that differs from its column's.
    fn rows(columns: &[Column], state: TaskState) -> Vec<(&str, u32, Option<TaskState>)> {
        let column = columns.iter().find(|column| column.state == state).expect("column");
        column
            .rows
            .iter()
            .map(|row| (row.title.as_str(), row.depth, Some(row.state).filter(|own| *own != state)))
            .collect()
    }

    #[test]
    fn same_named_workspaces_on_two_servers_form_one_section_of_the_first_picked_color() {
        let violet = Swatch { hue: Hue::Violet, tone: Tone::Vivid };
        let mut mac_mini = cache(
            &[("wsp_b", "octopus"), ("wsp_c", "dotfiles"), ("wsp_d", "scratch")],
            vec![
                task("tsk_1", "wsp_b", None, "a", TaskState::Pinned),
                task("tsk_2", "wsp_c", None, "c", TaskState::Completed),
            ],
        );
        mac_mini.workspaces.get_mut(&WorkspaceId("wsp_b".into())).expect("octopus").color = Color::Picked(violet);
        let caches = BTreeMap::from([
            (
                "laptop".into(),
                cache(&[("wsp_a", "octopus")], vec![task("tsk_1", "wsp_a", None, "b", TaskState::Pinned)]),
            ),
            ("mac-mini".into(), mac_mini),
        ]);
        let lists = lists(&caches, &Waking::new());
        let names: Vec<(&str, u32)> =
            lists.sections.iter().map(|section| (section.name.as_str(), section.live)).collect();
        assert_eq!(names, [("dotfiles", 0), ("octopus", 2), ("scratch", 0)]);
        let octopus = section(&lists, "octopus");
        let servers: Vec<&str> = octopus.groups[0].rows.iter().map(|row| row.at.server.as_str()).collect();
        assert_eq!(servers, ["mac-mini", "laptop"]);
        assert_eq!((octopus.swatch, section(&lists, "scratch").swatch), (violet, auto_swatch("scratch")));
        assert_eq!(lists.workspaces.len(), 4);
    }

    #[test]
    fn a_subtask_sits_under_its_parent_whatever_its_state_until_the_list_leaves_the_parent_out() {
        let mut archived = task("tsk_7", "wsp_a", None, "old", TaskState::Cancelled);
        archived.archived_at = Some(1);
        let caches = BTreeMap::from([(
            "mac-mini".into(),
            cache(
                &[("wsp_a", "octopus"), ("wsp_b", "acme")],
                vec![
                    task("tsk_1", "wsp_a", None, "protocol v1", TaskState::Pinned),
                    task("tsk_2", "wsp_a", Some("tsk_1"), "strict serde", TaskState::Pinned),
                    task("tsk_3", "wsp_a", Some("tsk_2"), "pairing", TaskState::Temporary),
                    task("tsk_4", "wsp_a", Some("tsk_1"), "crate layout", TaskState::Completed),
                    task("tsk_5", "wsp_a", None, "shipped", TaskState::Completed),
                    task("tsk_6", "wsp_a", Some("tsk_5"), "follow-up", TaskState::Pinned),
                    archived,
                    task("tsk_8", "wsp_a", Some("tsk_7"), "leftover", TaskState::Completed),
                    task("tsk_9", "wsp_b", Some("tsk_1"), "elsewhere", TaskState::Pinned),
                ],
            ),
        )]);
        let lists = lists(&caches, &Waking::new());
        let octopus = section(&lists, "octopus");
        assert_eq!(
            rows(&octopus.groups, TaskState::Pinned),
            [
                ("follow-up", 0, None),
                ("protocol v1", 0, None),
                ("strict serde", 1, None),
                ("pairing", 2, Some(TaskState::Temporary))
            ]
        );
        assert_eq!(rows(&octopus.groups, TaskState::Temporary), []);
        assert_eq!(
            rows(&octopus.columns, TaskState::Pinned),
            [
                ("protocol v1", 0, None),
                ("crate layout", 1, Some(TaskState::Completed)),
                ("strict serde", 1, None),
                ("pairing", 2, Some(TaskState::Temporary)),
            ]
        );
        assert_eq!(
            rows(&octopus.columns, TaskState::Completed),
            [("leftover", 0, None), ("shipped", 0, None), ("follow-up", 1, Some(TaskState::Pinned))]
        );
        assert_eq!(rows(&section(&lists, "acme").groups, TaskState::Pinned), [("elsewhere", 0, None)]);
    }

    #[test]
    fn auto_never_lands_on_a_status_or_accent_hue() {
        let excluded = [Hue::Red, Hue::Orange, Hue::Amber, Hue::Lime, Hue::Green, Hue::Mint, Hue::Indigo];
        let picks: Vec<Swatch> = (0..1000).map(|n| auto_swatch(&format!("workspace-{n}"))).collect();
        assert!(picks.iter().all(|swatch| swatch.tone == Tone::Soft && !excluded.contains(&swatch.hue)));
        assert!(
            AUTO_HUES.iter().all(|hue| picks.iter().any(|swatch| swatch.hue == *hue)),
            "auto spreads over every hue it may take"
        );
    }
}
