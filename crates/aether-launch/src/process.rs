//! Run a prepared launch plan as a child process. Never invokes a shell.
//! This module is intentionally not exposed through Tauri IPC.

use std::collections::HashSet;
use std::fs;
use std::io::{self, BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use thiserror::Error;

use crate::LaunchPlan;

pub const MAX_LOG_BYTES: usize = 8 * 1024;
pub const LOG_QUEUE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub stream: LogStream,
    pub line: String,
    pub truncated: bool,
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("Java executable must be an absolute path to an existing file")]
    InvalidExecutable,
    #[error("game working directory does not exist")]
    InvalidWorkingDirectory,
    #[error("cannot start or supervise game process: {0}")]
    Io(#[from] io::Error),
}

pub struct GameProcess {
    child: Child,
    logs: Receiver<LogRecord>,
    readers: Vec<JoinHandle<()>>,
    discarded: Arc<AtomicU64>,
    exit_status: Option<ExitStatus>,
}

impl GameProcess {
    /// Spawn the prepared Java executable directly, with exact argv values.
    /// Callers must supply extra values to redact if credentials occur beyond
    /// standard --accessToken or --access-token arguments.
    pub fn spawn(plan: &LaunchPlan, extra_redactions: &[String]) -> Result<Self, ProcessError> {
        let executable = &plan.runtime.executable;
        if !executable.is_absolute() || !executable.is_file() {
            return Err(ProcessError::InvalidExecutable);
        }
        if !plan.working_directory.is_dir() {
            return Err(ProcessError::InvalidWorkingDirectory);
        }

        let redactions = collect_redactions(&plan.arguments, extra_redactions);
        let mut child = Command::new(executable)
            .args(&plan.arguments)
            .current_dir(&plan.working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let (sender, logs) = mpsc::sync_channel(LOG_QUEUE_CAPACITY);
        let discarded = Arc::new(AtomicU64::new(0));
        let mut readers = Vec::new();
        if let Some(stdout) = child.stdout.take() {
            readers.push(spawn_reader(
                stdout,
                LogStream::Stdout,
                sender.clone(),
                Arc::clone(&discarded),
                redactions.clone(),
            ));
        }
        if let Some(stderr) = child.stderr.take() {
            readers.push(spawn_reader(
                stderr,
                LogStream::Stderr,
                sender,
                Arc::clone(&discarded),
                redactions,
            ));
        }

        Ok(Self {
            child,
            logs,
            readers,
            discarded,
            exit_status: None,
        })
    }

    #[must_use]
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Collect buffered messages without waiting for the child or blocking.
    #[must_use]
    pub fn drain_logs(&self) -> Vec<LogRecord> {
        self.logs.try_iter().collect()
    }

    /// Number of messages discarded due to backpressure.
    #[must_use]
    pub fn discarded_logs(&self) -> u64 {
        self.discarded.load(Ordering::Relaxed)
    }

    /// Poll without blocking; the returned status is stable after process exit.
    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, ProcessError> {
        if let Some(status) = self.exit_status {
            return Ok(Some(status));
        }
        let status = self.child.try_wait()?;
        self.exit_status = status;
        Ok(status)
    }

    /// Wait for the main process to exit and for its output readers to finish.
    /// The caller may call drain_logs afterward to consume remaining records.
    pub fn wait(&mut self) -> Result<ExitStatus, ProcessError> {
        if let Some(status) = self.exit_status {
            self.join_readers();
            return Ok(status);
        }
        let status = self.child.wait()?;
        self.exit_status = Some(status);
        self.join_readers();
        Ok(status)
    }

    /// Request forced termination of the main process (not its descendants).
    /// Idempotent if the process has already exited.
    pub fn stop(&mut self) -> Result<ExitStatus, ProcessError> {
        if let Some(status) = self.try_wait()? {
            self.join_readers();
            return Ok(status);
        }
        match self.child.kill() {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => {}
            Err(error) => return Err(ProcessError::Io(error)),
        }
        self.wait()
    }

    fn join_readers(&mut self) {
        for handle in self.readers.drain(..) {
            let _ = handle.join();
        }
    }
}

impl Drop for GameProcess {
    fn drop(&mut self) {
        if self.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        // Reader threads are detached here to avoid hanging if another
        // descendant process inherited a pipe handle.
    }
}

fn collect_redactions(arguments: &[String], extras: &[String]) -> Vec<String> {
    let mut values = HashSet::new();
    for item in extras {
        if !item.is_empty() {
            values.insert(item.clone());
        }
    }
    for pair in arguments.windows(2) {
        if matches!(
            pair[0].to_ascii_lowercase().as_str(),
            "--accesstoken" | "--access-token" | "--clienttoken" | "--client-token"
        ) && !pair[1].is_empty()
        {
            values.insert(pair[1].clone());
        }
    }
    // Longer secrets first so a short prefix does not interfere with masking.
    let mut values: Vec<String> = values.into_iter().collect();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values
}

fn spawn_reader<T: Read + Send + 'static>(
    reader: T,
    stream: LogStream,
    sender: SyncSender<LogRecord>,
    discarded: Arc<AtomicU64>,
    redactions: Vec<String>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut input = BufReader::new(reader);
        let mut line = Vec::new();
        let mut truncated = false;

        loop {
            let chunk = match input.fill_buf() {
                Ok(value) => value,
                Err(_) => break,
            };
            if chunk.is_empty() {
                break;
            }
            let mut count = 0;
            for &byte in chunk {
                count += 1;
                if byte == b'\n' {
                    publish(&sender, &discarded, stream, &line, truncated, &redactions);
                    line.clear();
                    truncated = false;
                } else if line.len() < MAX_LOG_BYTES {
                    line.push(byte);
                } else {
                    truncated = true;
                }
            }
            input.consume(count);
        }
        if !line.is_empty() || truncated {
            publish(&sender, &discarded, stream, &line, truncated, &redactions);
        }
    })
}

fn publish(
    sender: &SyncSender<LogRecord>,
    discarded: &AtomicU64,
    stream: LogStream,
    bytes: &[u8],
    truncated: bool,
    redactions: &[String],
) {
    let mut line = String::from_utf8_lossy(bytes).trim_end_matches('\r').to_owned();
    for secret in redactions {
        line = line.replace(secret, "[REDACTED]");
    }
    let record = LogRecord { stream, line, truncated };
    if let Err(TrySendError::Full(_)) = sender.try_send(record) {
        discarded.fetch_add(1, Ordering::Relaxed);
    }
}

/// Validate process paths before launch; kept internal to avoid arbitrary
/// executable selection via an IPC API.
pub fn validate_launch_executable(plan: &LaunchPlan) -> Result<PathBuf, ProcessError> {
    let path = fs::canonicalize(&plan.runtime.executable)
        .map_err(|_| ProcessError::InvalidExecutable)?;
    if !path.is_file() {
        return Err(ProcessError::InvalidExecutable);
    }
    if !plan.working_directory.is_dir() {
        return Err(ProcessError::InvalidWorkingDirectory);
    }
    Ok(path)
}

#[cfg(test)]
#[path = "process/process_tests.rs"]
mod tests;
