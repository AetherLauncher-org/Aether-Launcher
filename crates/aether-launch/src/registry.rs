//! In-memory game process lifecycle, keyed by a stable instance ID.
//!
//! Internal Rust API only. A frontend must never submit an executable or argv
//! directly: a trusted launch coordinator must build the plan for that instance.

use std::collections::HashMap;

use aether_core::{InstanceId, InstanceState};
use serde::Serialize;
use thiserror::Error;

use crate::LaunchPlan;
use crate::process::{GameProcess, LogRecord, ProcessError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceRunState {
    Running,
    Exited,
    Stopped,
    Failed,
}

impl InstanceRunState {
    /// Mirror running/ready/failed into the existing instance domain model.
    #[must_use]
    pub fn instance_state(self) -> InstanceState {
        match self {
            Self::Running => InstanceState::Running,
            Self::Exited | Self::Stopped => InstanceState::Ready,
            Self::Failed => InstanceState::Failed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceProcessStatus {
    pub instance_id: InstanceId,
    pub state: InstanceRunState,
    pub pid: u32,
    /// May be absent when the OS terminated a child with a signal.
    pub exit_code: Option<i32>,
    pub discarded_logs: u64,
}

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("this game instance already has a running process")]
    AlreadyRunning,
    #[error("no process is registered for the requested instance")]
    UnknownInstance,
    #[error("cannot remove a running game instance")]
    StillRunning,
    #[error(transparent)]
    Process(#[from] ProcessError),
}

struct RegisteredProcess {
    process: GameProcess,
    status: InstanceProcessStatus,
}

impl RegisteredProcess {
    fn poll(&mut self) -> Result<(), RegistryError> {
        if self.status.state == InstanceRunState::Running {
            if let Some(exit) = self.process.try_wait()? {
                self.status.state = if exit.success() {
                    InstanceRunState::Exited
                } else {
                    InstanceRunState::Failed
                };
                self.status.exit_code = exit.code();
            }
        }
        self.status.discarded_logs = self.process.discarded_logs();
        Ok(())
    }
}

/// A process registry is meant to be owned by the backend, with external
/// synchronization if multiple threads can call its mutable methods.
///
/// Keeping the underlying process handle for exited entries lets callers
/// drain trailing logs and inspect the final result before explicitly removing
/// the entry. The registry never exposes raw command execution to Tauri.
#[derive(Default)]
pub struct InstanceProcessRegistry {
    entries: HashMap<InstanceId, RegisteredProcess>,
}

impl InstanceProcessRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start one previously prepared, trusted plan for an instance. An instance
    /// cannot run twice concurrently, even if its existing entry is being polled.
    pub fn start(
        &mut self,
        instance_id: InstanceId,
        plan: &LaunchPlan,
        extra_redactions: &[String],
    ) -> Result<InstanceProcessStatus, RegistryError> {
        if let Some(entry) = self.entries.get_mut(&instance_id) {
            entry.poll()?;
            if entry.status.state == InstanceRunState::Running {
                return Err(RegistryError::AlreadyRunning);
            }
        }

        let process = GameProcess::spawn(plan, extra_redactions)?;
        let status = InstanceProcessStatus {
            instance_id,
            state: InstanceRunState::Running,
            pid: process.id(),
            exit_code: None,
            discarded_logs: 0,
        };
        self.entries.insert(
            instance_id,
            RegisteredProcess {
                process,
                status: status.clone(),
            },
        );
        Ok(status)
    }

    /// Refresh the child exit status without waiting or consuming logs.
    pub fn status(
        &mut self,
        instance_id: InstanceId,
    ) -> Result<InstanceProcessStatus, RegistryError> {
        let entry = self
            .entries
            .get_mut(&instance_id)
            .ok_or(RegistryError::UnknownInstance)?;
        entry.poll()?;
        Ok(entry.status.clone())
    }

    /// Refresh and list the statuses of all registered instances.
    pub fn list(&mut self) -> Result<Vec<InstanceProcessStatus>, RegistryError> {
        let mut statuses = Vec::with_capacity(self.entries.len());
        for entry in self.entries.values_mut() {
            entry.poll()?;
            statuses.push(entry.status.clone());
        }
        // HashMap iteration order is not stable; this is a consistent UI order.
        statuses.sort_by_key(|status| status.pid);
        Ok(statuses)
    }

    /// Read at most the process's bounded 256-record log buffer.
    /// Records are consumed; repeated calls return only newly arrived lines.
    pub fn drain_logs(&mut self, instance_id: InstanceId) -> Result<Vec<LogRecord>, RegistryError> {
        let entry = self
            .entries
            .get_mut(&instance_id)
            .ok_or(RegistryError::UnknownInstance)?;
        entry.poll()?;
        Ok(entry.process.drain_logs())
    }

    /// Force-stop only the tracked child. Does not stop descendants.
    /// Naturally exited children retain their success/failure status.
    pub fn stop(
        &mut self,
        instance_id: InstanceId,
    ) -> Result<InstanceProcessStatus, RegistryError> {
        let entry = self
            .entries
            .get_mut(&instance_id)
            .ok_or(RegistryError::UnknownInstance)?;
        entry.poll()?;
        if entry.status.state == InstanceRunState::Running {
            let exit = entry.process.stop()?;
            entry.status.state = InstanceRunState::Stopped;
            entry.status.exit_code = exit.code();
        }
        entry.status.discarded_logs = entry.process.discarded_logs();
        Ok(entry.status.clone())
    }

    /// Do not silently kill a running game to remove it from the registry.
    pub fn forget_finished(
        &mut self,
        instance_id: InstanceId,
    ) -> Result<InstanceProcessStatus, RegistryError> {
        let status = self.status(instance_id)?;
        if status.state == InstanceRunState::Running {
            return Err(RegistryError::StillRunning);
        }
        self.entries.remove(&instance_id);
        Ok(status)
    }
}

#[cfg(test)]
#[path = "registry/registry_tests.rs"]
mod tests;
