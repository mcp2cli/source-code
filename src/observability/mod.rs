//! Observability — `tracing` subscriber setup.
//!
//! A single [`ObservabilityHandle`] installs the global
//! `tracing-subscriber` on process start. Output destinations are
//! driven by config ([`crate::config::LoggingConfig`]):
//!
//! - **stderr** — human-readable `fmt` layer; default verbosity
//!   filtered by `RUST_LOG` / config.
//! - **file** — rolling JSON log to a path inside the runtime data
//!   dir; used by `mcp2cli daemon` for persistent diagnostics and by
//!   `mcp2cli doctor` to surface recent errors.
//!
//! Span-level context (request id, config name, transport kind) is
//! added by the layers so every MCP call carries enough context to
//! correlate logs with telemetry events.

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::{Context, Result};
use tracing_subscriber::{
    EnvFilter, fmt, fmt::MakeWriter, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::config::{LogFormat, LogOutput, LoggingConfig};

static LOGGING_INITIALIZED: OnceLock<()> = OnceLock::new();

#[derive(Default)]
pub struct ObservabilityHandle;

pub fn init(config: &LoggingConfig) -> Result<ObservabilityHandle> {
    if LOGGING_INITIALIZED.get().is_some() {
        return Ok(ObservabilityHandle);
    }

    let env_filter = EnvFilter::try_new(config.level.clone())
        .with_context(|| format!("invalid log filter: {}", config.level))?;
    let writer = MultiMakeWriter::from_config(config)?;

    match config.format {
        LogFormat::Json => tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .json()
                    .with_writer(writer)
                    .with_target(true)
                    .with_thread_names(true),
            )
            .try_init()
            .context("failed to initialize structured logging")?,
        LogFormat::Pretty => tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .with_writer(writer)
                    .with_target(true)
                    .with_thread_names(true),
            )
            .try_init()
            .context("failed to initialize logging")?,
    }

    let _ = LOGGING_INITIALIZED.set(());
    Ok(ObservabilityHandle)
}

#[derive(Clone)]
struct MultiMakeWriter {
    sinks: Arc<Vec<LogSink>>,
}

impl MultiMakeWriter {
    fn from_config(config: &LoggingConfig) -> Result<Self> {
        let mut sinks = Vec::new();
        for output in &config.outputs {
            sinks.push(LogSink::from_output(output)?);
        }
        Ok(Self {
            sinks: Arc::new(sinks),
        })
    }
}

impl<'a> MakeWriter<'a> for MultiMakeWriter {
    type Writer = MultiWriter;

    fn make_writer(&'a self) -> Self::Writer {
        MultiWriter {
            sinks: Arc::clone(&self.sinks),
        }
    }
}

struct MultiWriter {
    sinks: Arc<Vec<LogSink>>,
}

impl Write for MultiWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for sink in self.sinks.iter() {
            sink.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        for sink in self.sinks.iter() {
            sink.flush()?;
        }
        Ok(())
    }
}

#[derive(Clone)]
enum LogSink {
    Stdout,
    Stderr,
    File(Arc<Mutex<File>>),
}

impl LogSink {
    fn from_output(output: &LogOutput) -> Result<Self> {
        match output {
            LogOutput::Stdout => Ok(Self::Stdout),
            LogOutput::Stderr => Ok(Self::Stderr),
            LogOutput::File { path } => {
                if let Some(parent) = Path::new(path).parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create log directory: {}", parent.display())
                    })?;
                }
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .with_context(|| format!("failed to open log file: {path}"))?;
                Ok(Self::File(Arc::new(Mutex::new(file))))
            }
        }
    }

    fn write_all(&self, buf: &[u8]) -> io::Result<()> {
        match self {
            Self::Stdout => io::stdout().lock().write_all(buf),
            Self::Stderr => io::stderr().lock().write_all(buf),
            Self::File(file) => file.lock().expect("log file lock poisoned").write_all(buf),
        }
    }

    fn flush(&self) -> io::Result<()> {
        match self {
            Self::Stdout => io::stdout().lock().flush(),
            Self::Stderr => io::stderr().lock().flush(),
            Self::File(file) => file.lock().expect("log file lock poisoned").flush(),
        }
    }
}
