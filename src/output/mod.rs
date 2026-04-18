//! Structured output: [`CommandOutput`], [`OutputFormat`], and
//! renderers.
//!
//! Every command lowers its result to [`CommandOutput`] — a
//! discriminated union over "data | error | report" plus an envelope
//! carrying command metadata, exit status, and structured events.
//! The output layer then renders the envelope in one of three modes:
//!
//! - `OutputFormat::Human` — coloured, multi-line, for terminals.
//! - `OutputFormat::Json` — one JSON object per command, for scripts
//!   and `jq` pipelines.
//! - `OutputFormat::Ndjson` — newline-delimited JSON, each line a
//!   self-contained event. Useful for long-running operations and
//!   event-streaming sinks.
//!
//! Separating result construction from rendering means every command
//! path (static bridge, dynamic CLI, host commands) shares one
//! canonical representation — the `--json`/`--output ndjson` flags
//! behave identically everywhere.

use std::{
    ffi::OsString,
    io::{self, Write},
};

use anyhow::Result;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::runtime::RuntimeEvent;

/// Output format for command results.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
    Ndjson,
}

/// Structured command result with human-readable lines, summary, and JSON data.
#[derive(Debug, Clone, Serialize)]
pub struct CommandOutput {
    pub app_id: String,
    pub command: String,
    pub summary: String,
    pub lines: Vec<String>,
    pub data: Value,
}

impl CommandOutput {
    pub fn new(
        app_id: &str,
        command: &str,
        summary: String,
        lines: Vec<String>,
        data: Value,
    ) -> Self {
        Self {
            app_id: app_id.to_owned(),
            command: command.to_owned(),
            summary,
            lines,
            data,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub output_format: OutputFormat,
    pub output: CommandOutput,
}

pub fn detect_output_format(argv: &[OsString], default_format: OutputFormat) -> OutputFormat {
    let mut iter = argv.iter().peekable();
    while let Some(value) = iter.next() {
        let Some(as_str) = value.to_str() else {
            continue;
        };
        if as_str == "--json" {
            return OutputFormat::Json;
        }
        if let Some(raw) = as_str.strip_prefix("--output=")
            && let Some(format) = parse_output_format(raw)
        {
            return format;
        }
        if as_str == "--output"
            && let Some(next_value) = iter.peek().and_then(|next| next.to_str())
            && let Some(format) = parse_output_format(next_value)
        {
            return format;
        }
    }
    default_format
}

pub fn render(format: OutputFormat, output: &CommandOutput, events: &[RuntimeEvent]) -> Result<()> {
    match format {
        OutputFormat::Human => render_human(output),
        OutputFormat::Json => render_json(output),
        OutputFormat::Ndjson => render_ndjson(output, events),
    }
}

fn render_human(output: &CommandOutput) -> Result<()> {
    let mut stdout = io::stdout().lock();
    if output.lines.is_empty() {
        writeln!(stdout, "{}", output.summary)?;
    } else {
        for line in &output.lines {
            writeln!(stdout, "{}", line)?;
        }
    }
    Ok(())
}

fn render_json(output: &CommandOutput) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, output)?;
    writeln!(stdout)?;
    Ok(())
}

fn render_ndjson(output: &CommandOutput, events: &[RuntimeEvent]) -> Result<()> {
    let mut stdout = io::stdout().lock();
    for event in events {
        serde_json::to_writer(&mut stdout, &json!({ "type": "event", "event": event }))?;
        writeln!(stdout)?;
    }
    serde_json::to_writer(&mut stdout, &json!({ "type": "result", "result": output }))?;
    writeln!(stdout)?;
    Ok(())
}

fn parse_output_format(value: &str) -> Option<OutputFormat> {
    match value {
        "human" => Some(OutputFormat::Human),
        "json" => Some(OutputFormat::Json),
        "ndjson" => Some(OutputFormat::Ndjson),
        _ => None,
    }
}
