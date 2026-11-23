//! The command tree: one command per op, generated from the op's args, beside the verbs that send no op.

use anyhow::{Context, anyhow};
use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{Arg, ArgMatches, Command, FromArgMatches, Subcommand};
use octopus_proto::ops::{self, WorkspaceList};
use octopus_proto::schema::{Field, Shape};
use octopus_proto::types::WorkspaceId;
use octopus_proto::{Op, Outcome};
use serde_json::Value;

use crate::account::AccountVerb;
use crate::agent_hook::AgentVerb;
use crate::manual::{FileVerb, MicrophoneVerb, PairingVerb, ServerVerb, TermVerb};
use crate::skill::SkillVerb;
use crate::{call, connect, print, server_error};

pub(crate) const SERVER_FLAG: &str = "server";
pub(crate) const SERVER_HELP: &str = "The paired server's name.";

fn kebab(name: &str) -> String {
    name.replace('_', "-")
}

fn split_op(op: &str) -> (&str, &str) {
    op.split_once('.').expect("an op is named <noun>.<verb>")
}

fn op_name(noun: &str, verb: &str) -> String {
    format!("{noun}.{}", verb.replace('-', "_"))
}

fn sentence(doc: &str, addition: &str) -> String {
    format!("{}{addition}.", doc.trim_end_matches('.'))
}

fn help(field: &Field) -> String {
    match (&field.shape, field.shape.keyword()) {
        (Shape::Workspace, _) => sentence(field.doc, ", by name"),
        (_, Some((word, means))) => sentence(field.doc, &format!("; {word} for {means}")),
        _ => field.doc.to_string(),
    }
}

/// The JSON a flag's text stands for; a workspace stays its name until the call resolves it.
fn to_json(shape: &Shape, text: &str) -> Result<Value, String> {
    match shape {
        Shape::Text | Shape::Workspace | Shape::Keyword { .. } => Ok(Value::String(text.to_string())),
        Shape::Boolean => text.parse::<bool>().map(Value::Bool).map_err(|_| "pass true or false".to_string()),
        Shape::Integer => text.parse::<u64>().map(Value::from).map_err(|error| error.to_string()),
        Shape::Choice(values) if values.iter().any(|value| value == text) => Ok(Value::String(text.to_string())),
        Shape::Choice(values) => Err(format!("pass one of {}", values.join(", "))),
        Shape::Nullable { .. } if text == "none" => Ok(Value::Null),
        Shape::Nullable { value, .. } => to_json(value, text),
        Shape::List(_) | Shape::Record(_) => serde_json::from_str(text).map_err(|error| format!("pass JSON: {error}")),
    }
}

fn flag(field: &Field) -> Arg {
    let arg = Arg::new(field.name).long(kebab(field.name)).required(true).help(help(field));
    match &field.shape {
        Shape::Boolean => {
            arg.value_parser(PossibleValuesParser::new(["true", "false"]).map(|text| Value::Bool(text == "true")))
        }
        Shape::Choice(values) => arg.value_parser(PossibleValuesParser::new(values.clone()).map(Value::String)),
        shape => {
            let shape = shape.clone();
            arg.value_name(field.name).value_parser(move |text: &str| to_json(&shape, text))
        }
    }
}

pub(crate) fn server_flag() -> Arg {
    Arg::new(SERVER_FLAG).long(SERVER_FLAG).value_name(SERVER_FLAG).required(true).help(SERVER_HELP)
}

fn op_command(command: &ops::Command) -> Command {
    let (_, verb) = split_op(command.op);
    Command::new(kebab(verb)).about(command.doc).arg(server_flag()).args(command.fields.iter().map(flag))
}

/// Adds verbs to the noun's command, creating it on its first verbs.
fn extend(nouns: &mut Vec<Command>, noun: &str, verbs: impl FnOnce(Command) -> Command) {
    match nouns.iter_mut().find(|known| known.get_name() == noun) {
        Some(known) => *known = verbs(std::mem::take(known)),
        None => nouns.push(verbs(Command::new(noun.to_string()).subcommand_required(true))),
    }
}

pub fn tree() -> Command {
    let mut nouns: Vec<Command> = Vec::new();
    for command in ops::commands() {
        let (noun, _) = split_op(command.op);
        extend(&mut nouns, noun, |known| known.subcommand(op_command(&command)));
    }
    extend(&mut nouns, "agent", AgentVerb::augment_subcommands);
    extend(&mut nouns, "server", ServerVerb::augment_subcommands);
    extend(&mut nouns, "term", TermVerb::augment_subcommands);
    extend(&mut nouns, "file", FileVerb::augment_subcommands);
    extend(&mut nouns, "pairing", PairingVerb::augment_subcommands);
    extend(&mut nouns, "account", AccountVerb::augment_subcommands);
    extend(&mut nouns, "skill", SkillVerb::augment_subcommands);
    extend(&mut nouns, "microphone", MicrophoneVerb::augment_subcommands);
    nouns.insert(0, Command::new("uninstall").about(
        "Remove this machine's server's login service; its terminals and agents keep running until the machine stops.",
    ));
    nouns.insert(0, Command::new("install").about(
        "Install this machine's server as a login service, from the installed binary when it is newer and from this one otherwise.",
    ));
    #[cfg(feature = "desktop")]
    nouns.insert(0, Command::new("app").about("Open the window onto every server in this client's list."));
    Command::new("octopus")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Drive octopus servers. Every flag is required, and a success prints JSON.")
        .subcommand_required(true)
        .subcommands(nouns)
}

/// `error` lists each flag as `--parent <parent>`; a flag with a keyword says what the keyword means.
pub(crate) fn missing_flags(error: &clap::Error, argv: &[String]) -> Option<String> {
    if error.kind() != ErrorKind::MissingRequiredArgument {
        return None;
    }
    let Some(ContextValue::Strings(missing)) = error.get(ContextKind::InvalidArg) else { return None };
    let words: Vec<&str> = argv.iter().skip(1).map(String::as_str).take_while(|word| !word.starts_with('-')).collect();
    let op = words.get(..2).map(|path| op_name(path[0], path[1]));
    let fields =
        ops::commands().into_iter().find(|command| Some(command.op) == op.as_deref()).map(|command| command.fields);
    let named = missing.iter().map(|missing| {
        let flag = missing.split_whitespace().next().unwrap_or(missing);
        let keyword = fields
            .iter()
            .flatten()
            .find(|field| format!("--{}", kebab(field.name)) == flag)
            .and_then(|field| field.shape.keyword());
        match keyword {
            Some((word, means)) => format!("{flag} (pass {word} for {means})"),
            None => flag.to_string(),
        }
    });
    Some(format!("missing required flags: {}", named.collect::<Vec<_>>().join(", ")))
}

async fn workspace_id(connection: &octopus_client::Connection, name: &str) -> anyhow::Result<WorkspaceId> {
    let workspaces = call(connection, &WorkspaceList {}).await?.workspaces;
    let found = workspaces.into_iter().find(|workspace| workspace.name == name);
    found.map(|workspace| workspace.id).ok_or_else(|| anyhow!("no workspace named {name}"))
}

async fn call_op(command: &ops::Command, matches: &ArgMatches) -> anyhow::Result<()> {
    let server = matches.get_one::<String>(SERVER_FLAG).expect("--server is required");
    let connection = connect(server).await?;
    let mut args = serde_json::Map::new();
    for field in &command.fields {
        let value = matches.get_one::<Value>(field.name).expect("every flag is required").clone();
        let value = match (&field.shape, value) {
            (Shape::Workspace, Value::String(name)) => Value::String(workspace_id(&connection, &name).await?.0),
            (_, value) => value,
        };
        args.insert(field.name.to_string(), value);
    }
    match connection.call_raw(command.op, Value::Object(args)).await.context(command.op)? {
        Outcome::Ok(result) if command.op == ops::PairingOffer::NAME => {
            let offer: ops::Offer = serde_json::from_value(result.clone()).context("decode the offer")?;
            eprintln!("{}", octopus_client::qr::text(&offer.invite)?);
            print(&result)
        }
        Outcome::Ok(result) => print(&result),
        Outcome::Err(error) => Err(server_error(&error)),
    }
}

pub async fn run(matches: &ArgMatches) -> anyhow::Result<()> {
    let (noun, verbs) = matches.subcommand().expect("the tree requires a noun");
    let (verb, flags) = verbs.subcommand().expect("every noun requires a verb");
    let op = op_name(noun, verb);
    if let Some(command) = ops::commands().into_iter().find(|command| command.op == op) {
        return call_op(&command, flags).await;
    }
    match noun {
        "agent" => AgentVerb::from_arg_matches(verbs)?.run().await,
        "server" => ServerVerb::from_arg_matches(verbs)?.run().await,
        "term" => TermVerb::from_arg_matches(verbs)?.run().await,
        "file" => FileVerb::from_arg_matches(verbs)?.run().await,
        "pairing" => PairingVerb::from_arg_matches(verbs)?.run().await,
        "account" => AccountVerb::from_arg_matches(verbs)?.run().await,
        "skill" => SkillVerb::from_arg_matches(verbs)?.run(),
        _ => unreachable!("the tree declares no verb {noun} {verb}"),
    }
}
