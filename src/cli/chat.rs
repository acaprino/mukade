//! `mukade chat`: the lifecycle of the shared local service, and its operations as commands.
//!
//! Every operation is a call to the running service: this module holds no chat rule of its own.
//! The flags of an operation are its tool schema (`chat_tool_definitions`), so the command line
//! and the MCP tools cannot disagree: `room_id` is `--room-id`, a boolean is a bare flag, a list
//! is given comma-separated or repeated.

use crate::bus::{ChatError, chat_tool_definitions};
use crate::chat::client::ChatClient;
use crate::chat::{lifecycle, security};
use serde_json::{Map, Value, json};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const USAGE: &str = "\
mukade chat: one shared local chat service for harness instances

  mukade chat start [--port <n>]      start the background service
  mukade chat status                  running | stopped | unreachable, as JSON
  mukade chat stop                    stop it; rooms and messages stay
  mukade chat serve [--port <n>]      run it in the foreground (Ctrl-C stops it)
  mukade chat tools                   every operation with its arguments, as JSON
  mukade chat connect <harness> [--dry-run]     add the running service to the harness's
                                      own MCP servers (user scope); no credential is shown
  mukade chat disconnect <harness> [--dry-run]  take that entry out again
  mukade chat trust <harness> [--dry-run]
                                      trust the launch hooks once, for a harness that runs a
                                      hook only when trusted (codex); no other hook is touched
  mukade chat launch <harness> --room <name> [--role peer|lead|worker|observer]
                     [--listen <seconds>] [--max-wakes <n>] [-- <harness arguments>]
                                      start the harness already in the room (claude_code,
                                      codex, opencode, pi): its hooks or plugin hand it what
                                      the room writes; --listen keeps it waiting for messages
                                      addressed to it after each turn
  mukade chat <operation> [--<argument> <value>]... [--json] [--output <file>]

Operations: open close reopen delete rooms join rejoin leave members candidates bind unbind
send signal read peek wait ack observe ledger export.

An argument is the operation's field with dashes: room_id is --room-id. A boolean is a bare
flag (--latest). A list is comma-separated (--recipients codex-1,pi-1). --text-file <file|->
reads a message from a file or standard input. A change takes --request-id <uuid>; one is made
and printed on stderr when it is not given, so an uncertain call can be repeated safely.
read, wait and observe acknowledge what they printed only after the output was written.
";

/// One parsed `mukade chat` command line.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Help,
    Tools,
    Start {
        port: Option<u16>,
    },
    Status,
    Stop,
    Serve {
        port: Option<u16>,
        parent_id: Option<String>,
    },
    /// Write (`remove: false`) or take out the service's entry in a harness's MCP configuration.
    Connect {
        harness: crate::HarnessId,
        remove: bool,
        dry_run: bool,
    },
    /// Start a harness as a member of a room, with the adapter's hooks.
    Launch(Launch),
    /// Trust the adapter's hooks in a harness that runs a hook only once trusted.
    Trust {
        harness: crate::HarnessId,
        dry_run: bool,
    },
    /// One hook call of a launched harness: the event's JSON on standard input.
    Hook {
        harness: crate::HarnessId,
        event: crate::chat::adapter::Event,
    },
    Domain(DomainCommand),
}

/// `mukade chat launch`: which harness, into which room, and how it listens.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub harness: crate::HarnessId,
    pub room: String,
    pub role: String,
    pub listen: u64,
    pub max_wakes: Option<u32>,
    /// Passed to the harness as they are (after `--`).
    pub harness_args: Vec<String>,
}

/// One operation of the service, ready to be called.
#[derive(Debug, Clone, PartialEq)]
pub struct DomainCommand {
    pub operation: String,
    pub arguments: Value,
    /// One compact JSON object per line instead of the indented form.
    pub json: bool,
    /// `export` only: the file the whole snapshot is written to.
    pub output: Option<PathBuf>,
}

fn usage(message: impl Into<String>) -> ChatError {
    ChatError {
        code: "invalid_arguments".into(),
        message: message.into(),
        retryable: false,
        ack_batch_id: None,
    }
}

fn failed(code: &str, message: impl Into<String>) -> ChatError {
    ChatError {
        code: code.into(),
        message: message.into(),
        retryable: false,
        ack_batch_id: None,
    }
}

fn value_of<'a>(args: &'a [String], i: &mut usize, flag: &str) -> Result<&'a str, ChatError> {
    *i += 1;
    args.get(*i)
        .map(String::as_str)
        .ok_or_else(|| usage(format!("{flag} needs a value")))
}

fn port_of(text: &str) -> Result<u16, ChatError> {
    text.parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| usage("--port takes a number from 1 to 65535"))
}

/// The schema of one field, without the `anyOf […, null]` wrapper of an optional one.
fn plain_schema(schema: &Value) -> &Value {
    schema
        .get("anyOf")
        .and_then(Value::as_array)
        .and_then(|options| options.iter().find(|o| o["type"] != "null"))
        .unwrap_or(schema)
}

fn typed(flag: &str, schema: &Value, text: &str) -> Result<Value, ChatError> {
    match plain_schema(schema)["type"].as_str() {
        Some("integer") => text
            .parse::<u64>()
            .map(Value::from)
            .map_err(|_| usage(format!("{flag} takes a whole number"))),
        Some("boolean") => match text {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(usage(format!("{flag} takes true or false"))),
        },
        Some("array") => Ok(Value::Array(
            text.split(',')
                .filter(|part| !part.is_empty())
                .map(Value::from)
                .collect(),
        )),
        _ => {
            if plain_schema(schema)["format"] == "uuid" && uuid::Uuid::parse_str(text).is_err() {
                return Err(usage(format!("{flag} takes a UUID")));
            }
            Ok(Value::from(text))
        }
    }
}

/// Parse the arguments after `mukade chat`. Nothing is read or written: a mistyped command
/// creates no file.
pub fn parse(args: &[String]) -> Result<Command, ChatError> {
    let Some((name, rest)) = args.split_first() else {
        return Ok(Command::Help);
    };
    match name.as_str() {
        "-h" | "--help" | "help" => return Ok(Command::Help),
        "tools" | "status" | "stop" => {
            if !rest.is_empty() {
                return Err(usage(format!("chat {name} takes no arguments")));
            }
            return Ok(match name.as_str() {
                "tools" => Command::Tools,
                "status" => Command::Status,
                _ => Command::Stop,
            });
        }
        "trust" => {
            let mut harness = None;
            let mut dry_run = false;
            for arg in rest {
                match arg.as_str() {
                    "--dry-run" => dry_run = true,
                    other if other.starts_with("--") => {
                        return Err(usage(format!("unknown option {other} for chat trust")));
                    }
                    other if harness.is_none() => {
                        harness = Some(
                            other
                                .parse::<crate::HarnessId>()
                                .map_err(|_| usage(format!("unknown harness {other}")))?,
                        );
                    }
                    other => return Err(usage(format!("unexpected argument {other}"))),
                }
            }
            return Ok(Command::Trust {
                harness: harness.ok_or_else(|| usage("chat trust needs a harness"))?,
                dry_run,
            });
        }
        "hook" => {
            let [harness, event] = rest else {
                return Err(usage("chat hook takes a harness and an event"));
            };
            let harness = harness
                .parse::<crate::HarnessId>()
                .map_err(|_| usage(format!("unknown harness {harness}")))?;
            return Ok(Command::Hook {
                harness,
                event: event.parse()?,
            });
        }
        "launch" => {
            let (ours, theirs) = match rest.iter().position(|a| a == "--") {
                Some(at) => (&rest[..at], rest[at + 1..].to_vec()),
                None => (rest, Vec::new()),
            };
            let mut harness = None;
            let (mut room, mut role, mut listen, mut max_wakes) =
                (None, "peer".to_string(), 0u64, None);
            let mut i = 0;
            while i < ours.len() {
                match ours[i].as_str() {
                    "--room" => room = Some(value_of(ours, &mut i, "--room")?.to_string()),
                    "--role" => {
                        role = value_of(ours, &mut i, "--role")?.to_string();
                        if !matches!(role.as_str(), "peer" | "lead" | "worker" | "observer") {
                            return Err(usage("--role takes peer, lead, worker or observer"));
                        }
                    }
                    "--listen" => {
                        listen = value_of(ours, &mut i, "--listen")?
                            .parse()
                            .ok()
                            .filter(|s| *s <= crate::chat::adapter::MAX_LISTEN.as_secs())
                            .ok_or_else(|| usage("--listen takes seconds, at most 3600"))?;
                    }
                    "--max-wakes" => {
                        max_wakes = Some(
                            value_of(ours, &mut i, "--max-wakes")?
                                .parse()
                                .map_err(|_| usage("--max-wakes takes a whole number"))?,
                        );
                    }
                    other if other.starts_with("--") => {
                        return Err(usage(format!("unknown option {other} for chat launch")));
                    }
                    other if harness.is_none() => {
                        harness = Some(
                            other
                                .parse::<crate::HarnessId>()
                                .map_err(|_| usage(format!("unknown harness {other}")))?,
                        );
                    }
                    other => {
                        return Err(usage(format!(
                            "unexpected argument {other}; arguments for the harness go after --"
                        )));
                    }
                }
                i += 1;
            }
            return Ok(Command::Launch(Launch {
                harness: harness.ok_or_else(|| usage("chat launch needs a harness"))?,
                room: room.ok_or_else(|| usage("chat launch needs --room <name>"))?,
                role,
                listen,
                max_wakes,
                harness_args: theirs,
            }));
        }
        "connect" | "disconnect" => {
            let mut harness = None;
            let mut dry_run = false;
            for arg in rest {
                match arg.as_str() {
                    "--dry-run" => dry_run = true,
                    "--project" => {
                        return Err(usage(
                            "project scope is not written: the credential would sit in a file that can be committed; the user scope serves every project",
                        ));
                    }
                    other if other.starts_with("--") => {
                        return Err(usage(format!("unknown option {other} for chat {name}")));
                    }
                    other if harness.is_none() => {
                        harness = Some(other.parse::<crate::HarnessId>().map_err(|_| {
                            usage(format!(
                                "unknown harness {other}; one of claude_code, codex, opencode, pi"
                            ))
                        })?);
                    }
                    other => return Err(usage(format!("unexpected argument {other}"))),
                }
            }
            return Ok(Command::Connect {
                harness: harness.ok_or_else(|| usage(format!("chat {name} needs a harness")))?,
                remove: name == "disconnect",
                dry_run,
            });
        }
        "start" | "serve" => {
            let (mut port, mut parent_id) = (None, None);
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--port" => port = Some(port_of(value_of(rest, &mut i, "--port")?)?),
                    "--parent-id" if name == "serve" => {
                        let id = value_of(rest, &mut i, "--parent-id")?;
                        uuid::Uuid::parse_str(id).map_err(|_| usage("--parent-id takes a UUID"))?;
                        parent_id = Some(id.to_string());
                    }
                    other => return Err(usage(format!("unknown option {other} for chat {name}"))),
                }
                i += 1;
            }
            return Ok(if name == "start" {
                Command::Start { port }
            } else {
                Command::Serve { port, parent_id }
            });
        }
        _ => {}
    }

    let tool = chat_tool_definitions()
        .into_iter()
        .find(|tool| tool["name"].as_str() == Some(&format!("chat_{name}")))
        .ok_or_else(|| {
            usage(format!(
                "unknown chat command {name}; see mukade chat --help"
            ))
        })?;
    let properties = tool["inputSchema"]["properties"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut arguments = Map::new();
    let (mut json_output, mut output) = (false, None);
    let mut i = 0;
    while i < rest.len() {
        let flag = rest[i].as_str();
        match flag {
            "--json" => json_output = true,
            "--output" if name == "export" => {
                output = Some(PathBuf::from(value_of(rest, &mut i, flag)?));
            }
            "--text-file" if properties.contains_key("text") => {
                let source = value_of(rest, &mut i, flag)?;
                let mut text = String::new();
                if source == "-" {
                    io::stdin().read_to_string(&mut text).map(|_| ())
                } else {
                    std::fs::File::open(source)
                        .and_then(|mut f| f.read_to_string(&mut text).map(|_| ()))
                }
                .map_err(|_| usage("--text-file could not be read as UTF-8 text"))?;
                arguments.insert("text".into(), Value::from(text));
            }
            _ => {
                let key = flag
                    .strip_prefix("--")
                    .map(|k| k.replace('-', "_"))
                    .ok_or_else(|| usage(format!("unexpected argument {flag}")))?;
                let schema = properties
                    .get(&key)
                    .ok_or_else(|| usage(format!("unknown option {flag} for chat {name}")))?;
                let is_flag = plain_schema(schema)["type"] == "boolean"
                    && !matches!(rest.get(i + 1).map(String::as_str), Some("true" | "false"));
                let value = if is_flag {
                    Value::Bool(true)
                } else {
                    typed(flag, schema, value_of(rest, &mut i, flag)?)?
                };
                // A list may be repeated; anything else is given once.
                match (arguments.get_mut(&key), value) {
                    (Some(Value::Array(have)), Value::Array(more)) => have.extend(more),
                    (Some(_), _) => return Err(usage(format!("{flag} is given twice"))),
                    (None, value) => {
                        arguments.insert(key, value);
                    }
                }
            }
        }
        i += 1;
    }
    Ok(Command::Domain(DomainCommand {
        operation: name.clone(),
        arguments: Value::Object(arguments),
        json: json_output,
        output,
    }))
}

/// Whether text may reach a terminal as it is: no control character, no direction or
/// formatting mark that could reorder or hide what is shown.
fn shown_raw(c: char) -> bool {
    !(c.is_control()
        || matches!(c,
            '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}' | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}'))
}

/// Write one value so that a terminal is safe and a JSON parser still reads the same text:
/// every control or direction character is a `\uXXXX` escape. Such a character can only sit
/// inside a JSON string, where the escape means the character itself.
pub fn write_value(out: &mut dyn Write, value: &Value, compact: bool) -> io::Result<()> {
    let text = if compact {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    }
    .map_err(io::Error::other)?;
    let mut safe = String::with_capacity(text.len());
    for c in text.chars() {
        if shown_raw(c) || (c == '\n' && !compact) {
            safe.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                safe.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out.write_all(safe.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

/// Whether the operation stages a batch that is acknowledged once its output was written.
fn consumes(operation: &str) -> bool {
    matches!(operation, "read" | "wait" | "observe")
}

/// Run one operation against the service and write its result.
///
/// A change made without `--request-id` gets one, named on `err` before the call so a caller
/// who never sees the answer can repeat it. `read`, `wait` and `observe` acknowledge their
/// batch only after `out` took the whole result: a broken pipe leaves the batch pending.
pub async fn execute(
    client: &ChatClient,
    command: &DomainCommand,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), ChatError> {
    let mut arguments = command.arguments.clone();
    let tool = chat_tool_definitions()
        .into_iter()
        .find(|tool| tool["name"].as_str() == Some(&format!("chat_{}", command.operation)));
    let needs_request_id = tool.is_some_and(|tool| {
        tool["inputSchema"]["required"]
            .as_array()
            .is_some_and(|required| required.iter().any(|name| name == "request_id"))
    });
    if needs_request_id && arguments.get("request_id").is_none() {
        let id = uuid::Uuid::new_v4().to_string();
        let _ = writeln!(err, "request_id: {id}");
        arguments["request_id"] = json!(id);
    }
    if let Some(path) = &command.output {
        return export_to(client, arguments, path, out, command.json).await;
    }
    let result = client.call(&command.operation, arguments.clone()).await?;
    write_value(out, &result, command.json).map_err(|_| {
        failed(
            "output_failure",
            "The result could not be written; nothing was acknowledged",
        )
    })?;
    if consumes(&command.operation)
        && let Some(batch) = result["batch_id"].as_str()
    {
        let ack = json!({"room_id": arguments["room_id"], "member_handle": arguments["member_handle"], "batch_id": batch});
        client.call("ack", ack).await.map_err(|error| ChatError {
            code: "transport_failure".into(),
            message: format!(
                "The result was written but batch {batch} was not acknowledged ({}); it will be delivered again",
                error.message
            ),
            retryable: true,
            ack_batch_id: Some(batch.to_string()),
        })?;
    }
    Ok(())
}

/// Write the whole export snapshot to `path`, one JSON object per line. The pages go to a
/// private temporary file beside it, which takes the name only when every page arrived; an
/// existing file is never replaced.
async fn export_to(
    client: &ChatClient,
    mut arguments: Value,
    path: &Path,
    out: &mut dyn Write,
    compact: bool,
) -> Result<(), ChatError> {
    let refused = |what: &str| failed("output_failure", what.to_string());
    if path.exists() {
        return Err(refused(
            "The output file already exists; it is not replaced",
        ));
    }
    let folder = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = folder.join(format!(".mukade-export-{}.tmp", uuid::Uuid::new_v4()));
    let written = async {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| refused("The export could not be written beside the output file"))?;
        let mut pages = 0u64;
        loop {
            let page = client.call("export", arguments.clone()).await?;
            write_value(&mut file, &page, true)
                .map_err(|_| refused("The export could not be written"))?;
            pages += 1;
            match page["next_cursor"].as_str() {
                Some(cursor) => arguments["cursor"] = json!(cursor),
                None => break,
            }
        }
        file.sync_all()
            .map_err(|_| refused("The export could not be written"))?;
        drop(file);
        security::publish_new(&temporary, path)
            .map_err(|_| refused("The output file appeared meanwhile; it is not replaced"))?;
        Ok::<u64, ChatError>(pages)
    }
    .await;
    match written {
        Ok(pages) => write_value(
            out,
            &json!({"status": "ok", "output": path, "pages": pages}),
            compact,
        )
        .map_err(|_| refused("The export was written; its report could not be printed")),
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// `mukade chat …` from the command line.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let command = parse(args)?;
    let stdout = &mut io::stdout();
    match &command {
        Command::Help => {
            print!("{USAGE}");
            return Ok(());
        }
        Command::Tools => {
            for tool in chat_tool_definitions() {
                write_value(stdout, &tool, true)?;
            }
            return Ok(());
        }
        Command::Hook { harness, event } => {
            run_hook(*harness, *event);
            return Ok(());
        }
        Command::Trust { harness, dry_run } => {
            match run_trust(*harness, *dry_run) {
                Ok(report) => write_value(stdout, &report, true)?,
                Err(error) => {
                    eprintln!("{}: {}", error.code, error.message);
                    std::process::exit(error.exit_code());
                }
            }
            return Ok(());
        }
        _ => {}
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let paths = lifecycle::Paths::from_env()?;
    let result: Result<(), ChatError> = runtime.block_on(async {
        match command {
            Command::Start { port } => print_value(stdout, &lifecycle::start(&paths, port).await?),
            Command::Status => print_value(stdout, &lifecycle::status(&paths).await?),
            Command::Stop => print_value(stdout, &lifecycle::stop(&paths).await?),
            Command::Serve { port, parent_id } => {
                lifecycle::serve(paths.clone(), port, parent_id).await
            }
            Command::Connect {
                harness,
                remove,
                dry_run,
            } => {
                // The entry names the running service: its endpoint and credential are read
                // from the service's own protected folder, and the service must be running.
                let config = lifecycle::client_config(&paths)?;
                let server = crate::chat::connect::Server {
                    endpoint: config.endpoint,
                    token: config.token,
                    service_id: config.service_id,
                };
                let target = crate::chat::connect::target_of(
                    harness,
                    &crate::catalog::Vars::from_env(),
                    &server,
                )?;
                let act = if remove {
                    crate::chat::connect::disconnect
                } else {
                    crate::chat::connect::connect
                };
                print_value(
                    stdout,
                    &act(&target, &paths.bus_dir, harness.as_str(), dry_run)?,
                )
            }
            Command::Launch(launch) => run_launch(&paths, &launch).await,
            Command::Domain(domain) => {
                let client = ChatClient::connect(&lifecycle::client_config(&paths)?).await?;
                let done = execute(&client, &domain, stdout, &mut io::stderr()).await;
                client.close().await;
                done
            }
            Command::Help | Command::Tools | Command::Hook { .. } | Command::Trust { .. } => Ok(()),
        }
    });
    // A service past its shutdown deadline may still hold blocking database work: the process
    // that owns it ends here, with the failure reported, rather than waiting for ever.
    if let Err(error) = &result {
        let _ = io::stdout().flush();
        eprintln!("{}: {}", error.code, error.message);
        runtime.shutdown_timeout(std::time::Duration::from_secs(1));
        std::process::exit(error.exit_code());
    }
    Ok(())
}

fn print_value(out: &mut dyn Write, value: &Value) -> Result<(), ChatError> {
    write_value(out, value, true)
        .map_err(|_| failed("output_failure", "The result could not be written"))
}

/// One hook call. A hook never breaks its harness: it always exits 0, prints nothing when the
/// instance was not launched into a room, and says on stderr why it did nothing else.
fn run_hook(harness: crate::HarnessId, event: crate::chat::adapter::Event) {
    use crate::chat::adapter;
    let result = (|| -> Result<(), ChatError> {
        let Some(settings) = adapter::Settings::from_env()? else {
            return Ok(());
        };
        let mut text = String::new();
        io::stdin()
            .take(1 << 20)
            .read_to_string(&mut text)
            .map_err(|_| usage("the hook input could not be read"))?;
        let value: Value =
            serde_json::from_str(&text).map_err(|_| usage("the hook input is not JSON"))?;
        let input = adapter::HookInput::from_json(&value)?;
        let paths = lifecycle::Paths::from_env()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| failed("transport_failure", "no runtime for the hook"))?;
        runtime.block_on(async {
            let client = ChatClient::connect(&lifecycle::client_config(&paths)?).await?;
            // A plugin stops a listening hook when the user acts: the client is closed then, so
            // the service ends the call it was waiting in instead of holding the stream.
            let delivery = tokio::select! {
                delivery = adapter::handle(
                    &client,
                    &paths.bus_dir,
                    harness.as_str(),
                    &settings,
                    event,
                    &input,
                ) => delivery,
                () = terminated() => {
                    client.close().await;
                    return Ok(());
                }
            };
            client.close().await;
            let Some(delivery) = delivery? else {
                return Ok(());
            };
            let Some(output) = crate::harnesses::get(harness).chat_hook_output(event, &delivery)
            else {
                return Ok(());
            };
            let mut out = io::stdout().lock();
            if writeln!(out, "{output}").and_then(|_| out.flush()).is_err() {
                // The harness did not take it: hand it over again next time.
                let _ =
                    adapter::forget_delivery(&paths.bus_dir, harness.as_str(), &input.session_id);
            }
            Ok(())
        })
    })();
    if let Err(error) = result {
        eprintln!("mukade chat hook: {}: {}", error.code, error.message);
    }
}

/// Resolves when this process is asked to terminate (SIGTERM); never on Windows, where a stopped
/// hook is simply gone and its last call ends with its slice.
async fn terminated() {
    #[cfg(unix)]
    if let Ok(mut signal) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        signal.recv().await;
        return;
    }
    std::future::pending::<()>().await
}

/// A folder path without Windows's `\\?\` prefix, as the harness and its hooks read it back.
fn plain_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// The room named `name`, opened when there is none.
async fn room_id_of(client: &ChatClient, name: &str) -> Result<String, ChatError> {
    for attempt in 0..2 {
        let mut cursor: Option<String> = None;
        loop {
            let mut args = json!({"limit": 500});
            if let Some(cursor) = &cursor {
                args["cursor"] = json!(cursor);
            }
            let page = client.call("rooms", args).await?;
            let found = page["items"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|room| room["name"].as_str() == Some(name));
            if let Some(room) = found {
                if !room["closed_at"].is_null() {
                    return Err(failed(
                        "room_closed",
                        format!(
                            "room {name} is closed; reopen it with mukade chat reopen --room-id {}",
                            room["room_id"].as_str().unwrap_or("?")
                        ),
                    ));
                }
                return Ok(room["room_id"].as_str().unwrap_or_default().to_string());
            }
            match page["next_cursor"].as_str() {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        let opened = client
            .call(
                "open",
                json!({"name": name, "request_id": uuid::Uuid::new_v4().to_string()}),
            )
            .await;
        match opened {
            Ok(room) => return Ok(room["room_id"].as_str().unwrap_or_default().to_string()),
            // Another launch opened it meanwhile: look again.
            Err(_) if attempt == 0 => continue,
            Err(error) => return Err(error),
        }
    }
    Err(failed("transport_failure", "the room could not be opened"))
}

/// Start the service if needed, find or open the room, write the harness's launch kit, and run
/// the harness in this terminal until it exits; its exit code is this command's.
async fn run_launch(paths: &lifecycle::Paths, launch: &Launch) -> Result<(), ChatError> {
    use crate::chat::adapter;
    // A harness that cannot print a hook's answer has no adapter: refused before anything starts.
    let adapter_of = crate::harnesses::get(launch.harness);
    if adapter_of
        .chat_hook_output(
            adapter::Event::Stop,
            &adapter::Delivery::Continue(String::new()),
        )
        .is_none()
    {
        return Err(failed(
            "unsupported",
            format!(
                "{} has no chat adapter yet; connect it with mukade chat connect and use the tools",
                launch.harness
            ),
        ));
    }
    lifecycle::start(paths, None).await?;
    let config = lifecycle::client_config(paths)?;
    let client = ChatClient::connect(&config).await?;
    let room_id = room_id_of(&client, &launch.room).await;
    client.close().await;
    let room_id = room_id?;
    let kit = adapter::Kit {
        exe: std::env::current_exe()
            .map_err(|_| failed("transport_failure", "the mukade executable cannot be found"))?,
        folder: paths
            .bus_dir
            .join("adapters")
            .join(launch.harness.as_str())
            .join(format!("launch-{}", uuid::Uuid::new_v4().simple())),
        server: crate::chat::connect::Server {
            endpoint: config.endpoint,
            token: config.token,
            service_id: config.service_id,
        },
        stop_timeout: adapter::STOP_HOOK_TIMEOUT,
        args: launch.harness_args.clone(),
    };
    let Some(mut command) = crate::harnesses::get(launch.harness).chat_launch(&kit)? else {
        return Err(failed(
            "unsupported",
            format!(
                "{} has no chat adapter yet; connect it with mukade chat connect and use the tools",
                launch.harness
            ),
        ));
    };
    command
        .env("MUKADE_DATA_DIR", plain_path(&paths.data_dir))
        .env(adapter::ENV_ROOM, &room_id)
        .env(adapter::ENV_ROLE, &launch.role)
        .env(adapter::ENV_LISTEN, launch.listen.to_string());
    if let Some(n) = launch.max_wakes {
        command.env(adapter::ENV_MAX_WAKES, n.to_string());
    }
    eprintln!(
        "mukade: {} joins room {} as {}{}",
        launch.harness,
        launch.room,
        launch.role,
        if launch.listen > 0 {
            format!(", listening {} s after each turn", launch.listen)
        } else {
            String::new()
        }
    );
    let status = command.status();
    let _ = std::fs::remove_dir_all(&kit.folder);
    let status = status.map_err(|_| {
        failed(
            "transport_failure",
            format!("{} could not be started", launch.harness),
        )
    })?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Trust the launch hooks of `harness`, as this mukade executable writes them. Nothing else of
/// the harness's trust is read or changed, and no service is needed.
fn run_trust(harness: crate::HarnessId, dry_run: bool) -> Result<Value, ChatError> {
    use crate::chat::adapter;
    let adapter_of = crate::harnesses::get(harness);
    let kit = adapter::Kit {
        exe: std::env::current_exe()
            .map_err(|_| failed("transport_failure", "the mukade executable cannot be found"))?,
        folder: PathBuf::new(),
        // Only the hooks matter here; the MCP entry is not part of what is trusted.
        server: crate::chat::connect::Server {
            endpoint: String::new(),
            token: String::new(),
            service_id: String::new(),
        },
        stop_timeout: adapter::STOP_HOOK_TIMEOUT,
        args: Vec::new(),
    };
    match adapter_of.chat_trust(&kit, dry_run)? {
        Some(report) => Ok(report),
        None if adapter_of
            .chat_hook_output(
                adapter::Event::Stop,
                &adapter::Delivery::Continue(String::new()),
            )
            .is_some() =>
        {
            Ok(json!({"status": "not_needed", "harness": harness.as_str()}))
        }
        None => Err(failed(
            "unsupported",
            format!("{harness} has no chat adapter yet"),
        )),
    }
}
