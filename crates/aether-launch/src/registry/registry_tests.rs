use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use aether_core::{InstanceId, InstanceState};
use aether_runtime::JavaRuntime;

use super::{InstanceProcessRegistry, InstanceRunState, RegistryError};
use crate::LaunchPlan;
use crate::process::LogStream;

// A cross-platform helper launched through the Rust test harness itself.
#[test]
fn fixture_child() {
    if !std::env::args().any(|arg| arg == "--exact") {
        return;
    }
    println!("registry stdout registry-secret");
    eprintln!("registry stderr registry-secret");
    thread::sleep(Duration::from_millis(1200));
}

#[test]
fn fixture_failed_child() {
    if !std::env::args().any(|arg| arg == "--exact") {
        return;
    }
    panic!("simulated game failure");
}

fn plan() -> LaunchPlan {
    LaunchPlan {
        runtime: JavaRuntime {
            executable: std::env::current_exe().unwrap(),
            major_version: 21,
            architecture: "test".to_owned(),
            vendor: None,
        },
        working_directory: std::env::current_dir().unwrap(),
        arguments: vec![
            "--exact".to_owned(),
            "registry::tests::fixture_child".to_owned(),
            "--nocapture".to_owned(),
        ],
    }
}

fn await_exit(
    registry: &mut InstanceProcessRegistry,
    instance: InstanceId,
) -> super::InstanceProcessStatus {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let status = registry.status(instance).unwrap();
        if status.state != InstanceRunState::Running {
            return status;
        }
        assert!(Instant::now() < deadline, "child did not finish in time");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn rejects_duplicate_start_and_forgetting_running_process() {
    let id = InstanceId::new();
    let mut registry = InstanceProcessRegistry::new();
    let first = registry.start(id, &plan(), &[]).unwrap();
    assert_eq!(first.state, InstanceRunState::Running);
    assert!(first.pid > 0);
    assert_eq!(first.state.instance_state(), InstanceState::Running);
    assert!(matches!(
        registry.start(id, &plan(), &[]),
        Err(RegistryError::AlreadyRunning)
    ));
    assert!(matches!(
        registry.forget_finished(id),
        Err(RegistryError::StillRunning)
    ));

    let stopped = registry.stop(id).unwrap();
    assert_eq!(stopped.state, InstanceRunState::Stopped);
    assert_eq!(stopped.pid, first.pid);
    assert_eq!(stopped.state.instance_state(), InstanceState::Ready);
    assert_eq!(registry.stop(id).unwrap(), stopped);
    assert_eq!(registry.forget_finished(id).unwrap(), stopped);
    assert!(matches!(registry.status(id), Err(RegistryError::UnknownInstance)));
}

#[test]
fn natural_exit_updates_status_and_keeps_trailing_redacted_logs() {
    let id = InstanceId::new();
    let mut registry = InstanceProcessRegistry::new();
    registry.start(id, &plan(), &["registry-secret".to_owned()]).unwrap();
    let ended = await_exit(&mut registry, id);
    assert_eq!(ended.state, InstanceRunState::Exited);
    assert_eq!(ended.state.instance_state(), InstanceState::Ready);
    assert_eq!(ended.exit_code, Some(0));

    let mut collected = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        collected.extend(registry.drain_logs(id).unwrap());
        if collected.iter().any(|record| record.stream == LogStream::Stdout)
            && collected.iter().any(|record| record.stream == LogStream::Stderr)
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(collected.iter().any(|record| {
        record.stream == LogStream::Stdout
            && record.line.contains("registry stdout [REDACTED]")
    }));
    assert!(collected.iter().any(|record| {
        record.stream == LogStream::Stderr
            && record.line.contains("registry stderr [REDACTED]")
    }));
    assert!(collected.iter().all(|record| !record.line.contains("registry-secret")));
    assert!(registry.drain_logs(id).unwrap().is_empty());
    assert_eq!(registry.forget_finished(id).unwrap().state, InstanceRunState::Exited);
}

#[test]
fn supports_independent_instances_and_restart_after_stop() {
    let first = InstanceId::new();
    let second = InstanceId::new();
    let mut registry = InstanceProcessRegistry::new();
    registry.start(first, &plan(), &[]).unwrap();
    registry.start(second, &plan(), &[]).unwrap();

    let statuses = registry.list().unwrap();
    assert_eq!(statuses.len(), 2);
    assert!(statuses.iter().all(|entry| entry.state == InstanceRunState::Running));
    assert_ne!(statuses[0].pid, statuses[1].pid);

    let first_stop = registry.stop(first).unwrap();
    assert_eq!(first_stop.state, InstanceRunState::Stopped);
    assert_eq!(registry.status(second).unwrap().state, InstanceRunState::Running);

    let restart = registry.start(first, &plan(), &[]).unwrap();
    assert_eq!(restart.state, InstanceRunState::Running);
    assert_ne!(restart.pid, first_stop.pid);

    registry.stop(first).unwrap();
    registry.stop(second).unwrap();
}

#[test]
fn nonzero_child_exit_maps_to_failed_instance_state() {
    let id = InstanceId::new();
    let mut failed_plan = plan();
    failed_plan.arguments[1] = "registry::tests::fixture_failed_child".to_owned();
    let mut registry = InstanceProcessRegistry::new();
    registry.start(id, &failed_plan, &[]).unwrap();

    let status = await_exit(&mut registry, id);
    assert_eq!(status.state, InstanceRunState::Failed);
    assert_eq!(status.state.instance_state(), InstanceState::Failed);
    assert_ne!(status.exit_code, Some(0));
    assert_eq!(registry.stop(id).unwrap().state, InstanceRunState::Failed);
    registry.forget_finished(id).unwrap();
}

#[test]
fn failed_start_does_not_leave_a_phantom_running_entry() {
    let id = InstanceId::new();
    let mut invalid = plan();
    invalid.runtime.executable = PathBuf::from("not-an-absolute-java-executable");
    let mut registry = InstanceProcessRegistry::new();

    assert!(matches!(
        registry.start(id, &invalid, &[]),
        Err(RegistryError::Process(_))
    ));
    assert!(registry.list().unwrap().is_empty());
    registry.start(id, &plan(), &[]).unwrap();
    registry.stop(id).unwrap();
}

#[test]
fn unknown_instances_cannot_be_stopped_or_drained() {
    let id = InstanceId::new();
    let mut registry = InstanceProcessRegistry::new();
    assert!(matches!(registry.stop(id), Err(RegistryError::UnknownInstance)));
    assert!(matches!(registry.drain_logs(id), Err(RegistryError::UnknownInstance)));
}
