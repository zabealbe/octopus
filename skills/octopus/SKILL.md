---
name: octopus
description: Drive octopus servers from a shell, covering workspaces, tasks, agent terminals, files, browser panels and pairing. Use when an agent or script creates, moves or inspects octopus tasks, types into their terminals, or opens a page in a task's browser panel.
---

# octopus

Every flag is required. A success prints the op's result as JSON on stdout; a failure prints `error: ` and the reason on stderr and exits non-zero. Inside a task terminal, pass the terminal's own server and task: `--server "$OCTOPUS_SERVER" --task "$OCTOPUS_TASK"`.

## `octopus app`

Open the window onto every server in this client's list.

## `octopus install`

Install this machine's server as a login service, from the installed binary when it is newer and from this one otherwise.

## `octopus uninstall`

Remove this machine's server's login service; its terminals and agents keep running until the machine stops.

## `octopus workspace add`

Add a workspace: a named place this server files tasks in.

- `--server <server>`: The paired server's name.
- `--name <name>`: The workspace's name, unique on its server.
- `--root <root>`: Where new tasks' folders start: a directory on the server's machine, as an absolute path.
- `--color auto|red-mist|orange-mist|amber-mist|lime-mist|green-mist|mint-mist|teal-mist|sky-mist|blue-mist|indigo-mist|violet-mist|pink-mist|red-soft|orange-soft|amber-soft|lime-soft|green-soft|mint-soft|teal-soft|sky-soft|blue-soft|indigo-soft|violet-soft|pink-soft|red-vivid|orange-vivid|amber-vivid|lime-vivid|green-vivid|mint-vivid|teal-vivid|sky-vivid|blue-vivid|indigo-vivid|violet-vivid|pink-vivid|red-deep|orange-deep|amber-deep|lime-deep|green-deep|mint-deep|teal-deep|sky-deep|blue-deep|indigo-deep|violet-deep|pink-deep`: The workspace's color: auto, or a <hue>-<tone> of the matrix.

## `octopus workspace list`

List the server's workspaces.

- `--server <server>`: The paired server's name.

## `octopus workspace rename`

Rename a workspace; it keeps its root and its tasks.

- `--server <server>`: The paired server's name.
- `--workspace <workspace>`: The workspace, by name.
- `--name <name>`: Its new name, unique on its server.

## `octopus workspace set-color`

Set a workspace's color; it keeps its name, root and tasks.

- `--server <server>`: The paired server's name.
- `--workspace <workspace>`: The workspace, by name.
- `--color auto|red-mist|orange-mist|amber-mist|lime-mist|green-mist|mint-mist|teal-mist|sky-mist|blue-mist|indigo-mist|violet-mist|pink-mist|red-soft|orange-soft|amber-soft|lime-soft|green-soft|mint-soft|teal-soft|sky-soft|blue-soft|indigo-soft|violet-soft|pink-soft|red-vivid|orange-vivid|amber-vivid|lime-vivid|green-vivid|mint-vivid|teal-vivid|sky-vivid|blue-vivid|indigo-vivid|violet-vivid|pink-vivid|red-deep|orange-deep|amber-deep|lime-deep|green-deep|mint-deep|teal-deep|sky-deep|blue-deep|indigo-deep|violet-deep|pink-deep`: Its new color: auto, or a <hue>-<tone> of the matrix.

## `octopus workspace remove`

Remove a workspace with no live task, with every task in it; their worktrees go and their branches stay.

- `--server <server>`: The paired server's name.
- `--workspace <workspace>`: The workspace, by name.

## `octopus folder get`

Show a folder on the server's machine, for a new task to pick: the git repo and branch it sits in and the folders inside it, without looking inside them. Waits for the OS when it guards the folder.

- `--server <server>`: The paired server's name.
- `--folder <folder>`: The folder, as an absolute path, or ~ for the home folder of the server's user.

## `octopus harness set`

Declare how a harness starts its agent.

- `--server <server>`: The paired server's name.
- `--id <id>`: The harness's id, which tasks name.
- `--argv <argv>`: The agent's argv as a JSON array of strings, e.g. ["claude"].
- `--resume <resume>`: The args appended to argv to resume the task's session, with {session} for its id, e.g. ["--resume","{session}"].
- `--env <env>`: The variables the agent's terminal exports beside every task terminal's, e.g. [{"name":"FORCE_COLOR","value":"1"}]. {files} in argv, resume and a value stands for the server's harnesses folder.
- `--install <install>`: The command that installs the agent, shown when it is missing.

## `octopus harness list`

List every harness the server knows: its built-in ones and the ones `harness.set` stored.

- `--server <server>`: The paired server's name.

## `octopus harness check`

Show whether a harness's program is installed on the server, and the command that installs it.

- `--server <server>`: The paired server's name.
- `--id <id>`: The harness's id, which tasks name.

## `octopus task create`

Create a task, which works in its folder for life.

- `--server <server>`: The paired server's name.
- `--workspace <workspace>`: The workspace the task is filed in, by name.
- `--folder <folder>`: The directory on the server's machine the task works in, as an absolute path.
- `--mode in_place|worktree|copy`: Where the task runs: in_place in the folder, or worktree or copy in the same folder inside a worktree of the folder's git repo, a copy with the repo's ignored files cloned in.
- `--title <title>`: The task's title.
- `--parent <parent>`: The parent task's id, a task of the same workspace; none for a root task.
- `--state pinned|temporary|completed|cancelled`: The task's state.
- `--harness <harness>`: The harness that runs the task's agent.
- `--session <session>`: An existing session's id for the agent to resume; fresh for a fresh session.

## `octopus task move`

Move a task to a state.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--state pinned|temporary|completed|cancelled`: The state to move it to.

## `octopus task set-parent`

Nest a task under another task of its workspace, or make it a root task; it keeps its state and its subtasks.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--parent <parent>`: The parent task's id; none for a root task.

## `octopus task set-workspace`

File a root task and all its subtasks in another workspace, changing nothing else.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--workspace <workspace>`: The workspace to file it in, by name.

## `octopus task rename`

Rename a task; a worktree task's branch keeps its name.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--title <title>`: The task's new title.

## `octopus task set-session`

Point a task's agent at an existing session, ending the agent running now; the next focus resumes it.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--session <session>`: The session to resume.

## `octopus task archive`

Archive an ended task, which takes it off the Tasks board until it moves again.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.

## `octopus task focus`

Focus a task, starting its agent if it has none.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--args <args>`: The args an agent this focus starts runs after its harness's argv, before its start or resume args, e.g. ["--model","opus"]; a running agent keeps the args it started with.

## `octopus task get`

Print a task.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.

## `octopus agent report-session`

Record the session a task's agent runs in, as the agent reports it, leaving the agent running; its next start resumes it.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--session <session>`: The session the agent runs in.

## `octopus agent hook`

Report the session_id of the agent hook's JSON on stdin as the task's session. Prints nothing and exits 0 whatever happens, printing a failure on stderr, so a report never blocks the agent.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.

## `octopus term open`

Open a shell panel in a live task: the user's login shell, in a folder of the task's directory.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--path <path>`: The directory the shell starts in, relative to the task's directory; . for the directory itself.

## `octopus term install-microphone`

Open a shell panel in a live task that runs `sudo octopus microphone install` on the task's server, where the user types the server's password; the panel closes when the install succeeds.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.

## `octopus term close`

Stop a shell panel and drop it with its scrollback.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--panel <panel>`: The shell panel's name or id.

## `octopus term send`

Type text into a terminal panel.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--panel <panel>`: The panel's name or id, e.g. agent.
- `--text <text>`: The text to type.
- `--enter true|false`: Whether Enter follows the text.
- `--take-lock true|false`: Whether to take the panel's write lock from its holder.

## `octopus term take-lock`

Take a terminal panel's write lock from its holder and resize it to this client's screen.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--panel <panel>`: The panel's name or id, e.g. agent.
- `--cols <cols>`: Columns.
- `--rows <rows>`: Rows.

## `octopus term resize`

Resize a terminal panel; only its lock holder may.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--panel <panel>`: The panel's name or id, e.g. agent.
- `--cols <cols>`: Columns.
- `--rows <rows>`: Rows.

## `octopus term attach`

Stream a terminal panel into this terminal; Ctrl-] detaches.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--panel <panel>`: The panel's name or id, e.g. agent.
- `--take-lock true|false`: Whether to take the panel's write lock and size it to this terminal.

## `octopus file list`

List a folder of a task's directory.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--path <path>`: The directory, relative to the task's directory; . for the directory itself.

## `octopus file write`

Create a file in a task's directory, or replace its content.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--path <path>`: The file, relative to the task's directory.
- `--content <content>`: The file's new content, as text.

## `octopus file create`

Create an empty file or directory in a task's directory, with the directories above it; fails conflict when the path exists.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--path <path>`: The new entry, relative to the task's directory.
- `--kind file|directory`: What to create.

## `octopus file move`

Move or rename a file or directory of a task's directory; fails conflict when the destination exists.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--from <from>`: The entry, relative to the task's directory.
- `--to <to>`: Its new path, relative to the task's directory.

## `octopus file copy`

Copy a file or directory of a task's directory, a directory with everything in it; fails conflict when the destination exists.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--from <from>`: The entry, relative to the task's directory.
- `--to <to>`: The copy's path, relative to the task's directory.

## `octopus file delete`

Delete a file or directory of a task's directory, a directory with everything in it.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--path <path>`: The entry, relative to the task's directory.
- `--trash true|false`: Whether it goes to the trash of the server's machine rather than being removed for good.

## `octopus file search`

Find each line of a task's directory that contains a text.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--pattern <pattern>`: The literal text to find.

## `octopus file read`

Write a file of a task's directory to stdout, byte for byte.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--path <path>`: The file, relative to the task's directory.

## `octopus file upload`

Create a file of a task's directory from stdin, byte for byte; fails conflict when the path exists.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--path <path>`: The new file, relative to the task's directory.

## `octopus browser open`

Show a page in one of a task's browser panels on every client that renders the task, loading it again when the panel already shows it.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--url <url>`: The page: an http or https URL, one on localhost:<port> reaching that port on the server's machine, or file:// and an absolute path on the server's machine.
- `--panel <panel>`: `new` for a new browser panel, `background` for a new one no client selects or loads until it is chosen, or an existing browser panel's id.

## `octopus browser report`

Record the page a client's browser panel navigated to, which no other client loads until the panel reopens.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--panel <panel>`: The browser panel's id.
- `--url <url>`: The page the browser panel shows.
- `--title <title>`: The page's title, empty when it has none.

## `octopus browser close`

Close one of a task's browser panels on every client.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--panel <panel>`: The browser panel's id.

## `octopus browser move`

Move one of a task's browser panels to another place among them, on every client.

- `--server <server>`: The paired server's name.
- `--task <task>`: The task's id.
- `--panel <panel>`: The browser panel's id.
- `--to <to>`: The index the browser panel takes among the task's browser panels, the first at 0.

## `octopus pairing offer`

Show a one-time invite that pairs one client with this server.

- `--server <server>`: The paired server's name.

## `octopus pairing list`

List the accounts this server trusts and their devices.

- `--server <server>`: The paired server's name.

## `octopus pairing forget`

Stop trusting an account, closing its devices' connections and freeing their locks; it pairs again only with a new invite.

- `--server <server>`: The paired server's name.
- `--account <account>`: The account's id.

## `octopus pairing accept`

Pair with a server on another machine using the invite its `pairing offer` showed, and list it under its name.

- `--invite <invite>`: The one-time invite `pairing offer` printed: octopus://<server key>?code=…&relay=….

## `octopus server version`

Report the version of the octopus binary the server runs.

- `--server <server>`: The paired server's name.

## `octopus server run`

Run this machine's server in the foreground.

- `--name <name>`: The server's name, which agents in its terminals see as OCTOPUS_SERVER.
- `--socket <socket>`: The Unix socket clients on this machine connect to.
- `--store <store>`: The SQLite file the server keeps its state in.
- `--worktrees <worktrees>`: Where task worktrees live: <worktrees>/<repo>/<task>.
- `--terminal-host <terminal-host>`: Socket of the terminal host, the process that keeps this server's terminals running across restarts; started if nothing answers there.
- `--udp <udp>`: The UDP address clients on other machines reach this server on directly: 0.0.0.0:0 for a free port the OS picks at each start, or a fixed one like 0.0.0.0:7300 to forward on a router. 0.0.0.0 takes IPv6 too, on the same port when it is fixed.
- `--relay <relay>`: The iroh relay this server is homed on, which clients on other machines dial it through: one of n0's, e.g. https://euc1-1.relay.n0.iroh.link./, or one of your own.
- `--microphone true|false`: Give each terminal a virtual microphone that its lock holder feeds.

## `octopus server add`

Remember a server on this machine's local socket under a name.

- `--name <name>`: The name to call the server by.
- `--socket <socket>`: The server's --socket.

## `octopus server rename`

Call a server by another name on every device of this account; the server is not told.

- `--server <server>`: The paired server's name.
- `--name <name>`: The name to call the server by.

## `octopus server forget`

Drop a server from this account's list on every device; the server keeps trusting the account.

- `--server <server>`: The paired server's name.

## `octopus account show`

Show this device's account: its id, its devices and the servers it lists.

## `octopus account link`

Make a link another device joins this account with, held by the named server for 10 minutes.

- `--server <server>`: The paired server's name.

## `octopus account join`

Join the account a link names, bringing this device's servers along.

- `--link <link>`: The link `account link` printed: octopus://<server key>?join=…&account=…&invite=…&relay=….

## `octopus account remove`

Remove another device from this account; it loses every server of the account.

- `--device <device>`: The device's name, as `account show` lists it.

## `octopus account leave`

Leave this account for a fresh one of this device's own, keeping its servers.

## `octopus account start-over`

Start a fresh account with no servers, as a device its account removed does.

## `octopus skill install`

Write the octopus skill, rendered from this octopus's commands, to ~/.claude/skills/octopus/SKILL.md for Claude Code.

## `octopus skill uninstall`

Remove the octopus skill from ~/.claude/skills.

## `octopus microphone install`

As root: on macOS install the Octopus Microphone, restarting the audio service; on Linux enable a sound server, or name what is missing.

## `octopus microphone uninstall`

As root: on macOS remove the Octopus Microphone, restarting the audio service; on Linux leave the sound server.
