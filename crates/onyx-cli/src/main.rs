//! The `onyx` command-line tool.
//!
//! Two audiences, deliberately separated the way git separates plumbing from porcelain.
//! Machine-facing output (`--json`) is a public interface that other tools shell out to:
//! it is additive-only, and anything else bumps `reportVersion`. Human-facing output is
//! free to change.
//!
//! Argument parsing is written out rather than pulled from a library. Two commands and
//! two flags do not justify a dependency tree, and this binary is meant to be small
//! enough that nobody thinks twice about installing it.

use std::io::Read;

use onyx_core::json::Value;
use onyx_core::json_object;
use std::process::ExitCode;

use onyx_core::REPORT_VERSION;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
onyx — Onyx tools

USAGE
    onyx validate <file|-> [--json] [--strict]
    onyx inspect  <file|->
    onyx migrate  <file|-> --to <version>

COMMANDS
    validate    Check a document against the specification
    inspect     Print what a document contains, without validating it
    migrate     Rewrite a document as another specification version

OPTIONS
    --json          Emit a machine-readable report (validate only)
    --strict        Treat warnings as failures when choosing the exit code
    --to <version>  Target specification version (migrate only)
    --              End of options; everything after is a file name
    -h, --help      Print this help
    -V, --version   Print the version

EXIT CODES
    0   the document conforms
    1   the document was read and does not conform
    2   the tool could not do its job

`-` reads standard input, so onyx composes in a pipeline.

Downgrading parks members the older version has no home for under a reserved extension
namespace rather than dropping them, so migrating back restores them exactly.
";

/// Exit codes are part of the interface. A script must be able to tell a non-conforming
/// document from a tool that could not run.
mod exit {
    /// The document conforms.
    pub const CONFORMING: u8 = 0;
    /// The document was read but does not conform.
    pub const NON_CONFORMING: u8 = 1;
    /// The tool could not do its job: unreadable file, bad arguments.
    pub const TOOL_ERROR: u8 = 2;
}

fn read(path: &str) -> std::io::Result<String> {
    if path == "-" {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        Ok(buffer)
    } else {
        std::fs::read_to_string(path)
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("onyx: {message}");
    ExitCode::from(exit::TOOL_ERROR)
}

/// A tool error, in the same shape as every other `--json` payload.
///
/// `conforming` and `accepted` are **null**, not false: the tool never reached a verdict,
/// and reporting one it did not reach is how a caller ends up believing a document was
/// examined and rejected when in truth the file could not even be opened.
fn fail_json(rule: &str, message: &str, strict: bool) -> ExitCode {
    let payload = json_object! {
        "reportVersion" => REPORT_VERSION,
        "conforming" => Value::Null,
        "accepted" => Value::Null,
        "strict" => strict,
        "specVersion" => Value::Null,
        "producer" => Value::Null,
        "findings" => vec![Value::Object(json_object! {
            "severity" => "error",
            "rule" => rule,
            "path" => "",
            "message" => message,
        })],
    };
    println!("{}", Value::Object(payload));
    eprintln!("onyx: {message}");
    ExitCode::from(exit::TOOL_ERROR)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("{HELP}");
        return ExitCode::from(exit::TOOL_ERROR);
    }
    // `--` ends the options. Everything after it is a file name, including one that looks
    // exactly like a flag — without this, a file called `--help` could not be named at all,
    // because every spelling of it printed the help instead.
    let end_of_options = args.iter().position(|arg| arg == "--");
    let options = &args[..end_of_options.unwrap_or(args.len())];

    if options.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{HELP}");
        return ExitCode::from(exit::CONFORMING);
    }
    if options.iter().any(|arg| arg == "-V" || arg == "--version") {
        println!("onyx {VERSION}");
        return ExitCode::from(exit::CONFORMING);
    }

    let command = args[0].as_str();
    if !matches!(command, "validate" | "inspect" | "migrate") {
        eprintln!("onyx: unknown command {command:?}");
        eprintln!("{HELP}");
        return ExitCode::from(exit::TOOL_ERROR);
    }

    let mut path: Option<&str> = None;
    let mut json = false;
    let mut strict = false;
    let mut target: Option<&str> = None;

    let mut index = 1;
    while index < args.len() {
        // Past the marker, nothing is a flag.
        if end_of_options.is_some_and(|stop| index > stop) {
            match args[index].as_str() {
                other if path.is_none() => {
                    path = Some(other);
                    index += 1;
                    continue;
                }
                other => return fail(&format!("unexpected argument {other:?}")),
            }
        }
        if end_of_options == Some(index) {
            index += 1;
            continue;
        }

        match args[index].as_str() {
            "--json" => json = true,
            "--strict" => strict = true,
            "--to" => {
                index += 1;
                match args.get(index) {
                    // Guard against `--to --json`, which would otherwise migrate to a
                    // version literally named "--json" and fail somewhere confusing.
                    Some(value) if !value.starts_with("--") => target = Some(value.as_str()),
                    _ => return fail("--to needs a version, for example --to 1.1.0"),
                }
            }
            other if other.starts_with("--") => return fail(&format!("unknown option {other}")),
            other if path.is_none() => path = Some(other),
            other => return fail(&format!("unexpected argument {other:?}")),
        }
        index += 1;
    }

    // Options belong to one command each. Silently ignoring a misplaced one is how a
    // caller ends up believing something happened that did not.
    match command {
        "inspect" if json || strict || target.is_some() => {
            return fail("`inspect` takes no options");
        }
        "validate" if target.is_some() => return fail("--to applies to `migrate`"),
        "migrate" if json || strict => return fail("--json and --strict apply to `validate`"),
        "migrate" if target.is_none() => {
            return fail("migrate needs a target version, for example --to 1.1.0");
        }
        _ => {}
    }

    let Some(path) = path else {
        return fail(&format!(
            "{command} needs a file, or `-` for standard input"
        ));
    };

    let text = match read(path) {
        Ok(text) => text,
        Err(error) => {
            let message = format!("cannot read {path}: {error}");
            return if json {
                fail_json("tool/unreadable-input", &message, strict)
            } else {
                fail(&message)
            };
        }
    };

    match command {
        "validate" => validate(&text, json, strict),
        "migrate" => migrate(&text, target.unwrap_or_default()),
        _ => inspect(&text),
    }
}

/// Rewrites a document as another specification version and prints it.
///
/// Writes to standard output rather than editing in place: a migration that silently
/// rewrites someone's diary file is not a tool anyone should have to trust.
fn migrate(text: &str, target: &str) -> ExitCode {
    // Checked before the document is even read. `--to nonsense` is a mistake in the
    // command, and exit 1 means "the document does not conform" — which would be a claim
    // about the user's file that this tool never actually made.
    let shaped = target.split('.').count() == 3
        && target
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    if !shaped {
        return fail(&format!(
            "--to needs a MAJOR.MINOR.PATCH version, for example --to 1.1.0; got {target:?}"
        ));
    }

    let document = match onyx_core::parse(text) {
        Ok(document) => document,
        Err(error) => {
            eprintln!("onyx: {error}");
            return ExitCode::from(exit::NON_CONFORMING);
        }
    };

    match onyx_core::migrate(&document, target) {
        Ok(migrated) => {
            println!("{}", onyx_core::to_string_pretty(&migrated));
            ExitCode::from(exit::CONFORMING)
        }
        Err(error) => {
            eprintln!("onyx: {error}");
            ExitCode::from(exit::NON_CONFORMING)
        }
    }
}

/// For display only. A document with no producer is malformed and the validator says so;
/// printing a placeholder is not the same as storing one.
fn producer_name(document: &onyx_core::Document) -> &str {
    document
        .producer
        .as_ref()
        .and_then(|producer| producer.name.as_deref())
        .unwrap_or("(absent)")
}

fn findings_of(report: &onyx_core::Report) -> Value {
    Value::Array(
        report
            .findings
            .iter()
            .map(|finding| {
                Value::Object(json_object! {
                    "severity" => finding.severity.name(),
                    "rule" => finding.rule,
                    "path" => finding.path.clone(),
                    "message" => finding.message.clone(),
                })
            })
            .collect(),
    )
}

fn print_findings(report: &onyx_core::Report) {
    for finding in &report.findings {
        let path = if finding.path.is_empty() {
            "/"
        } else {
            &finding.path
        };
        eprintln!("{}: {} at {path}", finding.severity.name(), finding.rule);
        eprintln!("  {}", finding.message);
    }
}

fn validate(text: &str, json: bool, strict: bool) -> ExitCode {
    let document = match onyx_core::parse(text) {
        Ok(document) => document,
        Err(error) => {
            // The identity gate failed, so there is no document to check further.
            if json {
                // Same members as the success payload. A caller should not have to
                // discover that half the contract disappears on the one path where its
                // tooling most wants a uniform shape; `specVersion` and `producer` are
                // null because the document could not be read far enough to know them.
                let report = json_object! {
                    "reportVersion" => REPORT_VERSION,
                    "conforming" => false,
                    "accepted" => false,
                    "strict" => strict,
                    "specVersion" => Value::Null,
                    "producer" => Value::Null,
                    "findings" => vec![Value::Object(json_object! {
                        "severity" => "error",
                        "rule" => error.rule(),
                        "path" => "",
                        "message" => error.to_string(),
                    })],
                };
                println!("{}", Value::Object(report));
            } else {
                eprintln!("not conforming: {error}");
            }
            return ExitCode::from(exit::NON_CONFORMING);
        }
    };

    let report = onyx_core::validate(&document);

    // Warnings do not make a document non-conforming: several of them flag things the
    // specification explicitly permits. `--strict` is for callers who would rather stop.
    let clean =
        report.is_conforming() && !(strict && report.count(onyx_core::Severity::Warning) > 0);

    if json {
        // Two members because they answer two questions. `conforming` is about the
        // document and the specification, and `--strict` is not the specification's
        // opinion — it is the caller's. `accepted` is this run's verdict and is what the
        // exit code reports, so a script reading the payload and a script reading `$?`
        // can no longer disagree.
        let payload = json_object! {
            "reportVersion" => REPORT_VERSION,
            "conforming" => report.is_conforming(),
            "accepted" => clean,
            "strict" => strict,
            "specVersion" => document.spec_version.clone(),
            "producer" => document.producer.as_ref().map(|p| p.name.clone()),
            "findings" => findings_of(&report),
        };
        println!("{}", Value::Object(payload));
    } else {
        print_findings(&report);
        if report.findings.is_empty() {
            println!(
                "conforming: Onyx {} from {}",
                document.spec_version,
                producer_name(&document)
            );
        } else {
            println!(
                "{}: {} error(s), {} warning(s), {} note(s)",
                match (report.is_conforming(), clean) {
                    (true, true) => "conforming with notes",
                    // Conforming, but this run rejected it anyway. Saying only "conforming"
                    // beside a non-zero exit reads as a bug in the tool.
                    (true, false) => "conforming, but rejected under --strict",
                    (false, _) => "not conforming",
                },
                report.count(onyx_core::Severity::Error),
                report.count(onyx_core::Severity::Warning),
                report.count(onyx_core::Severity::Info),
            );
        }
    }

    ExitCode::from(if clean {
        exit::CONFORMING
    } else {
        exit::NON_CONFORMING
    })
}

fn inspect(text: &str) -> ExitCode {
    match onyx_core::parse(text) {
        Ok(document) => {
            println!("specVersion  {}", document.spec_version);
            println!("producer     {}", producer_name(&document));
            println!(
                "exportedAt   {}",
                document.exported_at.as_deref().unwrap_or("(absent)")
            );
            println!(
                "timeZone     {}",
                document.time_zone.as_deref().unwrap_or("(absent)")
            );

            let days = document.days();
            let entries: usize = days
                .iter()
                .map(|day| day.entries.as_ref().map_or(0, |entries| entries.len()))
                .sum();
            println!("days         {}", days.len());
            println!("entries      {entries}");
            println!("measurements {}", document.body_measurements().len());

            // Namespaces are listed, never opened. Which vendor wrote a block is useful
            // to a person reading this; what is inside it is that vendor's business.
            if let Some(extensions) = &document.extensions {
                for namespace in extensions.keys() {
                    println!("extension    {namespace}");
                }
            }

            ExitCode::from(exit::CONFORMING)
        }
        Err(error) => {
            eprintln!("onyx: {error}");
            ExitCode::from(exit::NON_CONFORMING)
        }
    }
}
