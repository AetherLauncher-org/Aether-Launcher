use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use aether_runtime::JavaRuntime;

use super::{
    GameProcess, LogStream, ProcessError, collect_redactions, spawn_reader, LOG_QUEUE_CAPACITY,
    MAX_LOG_BYTES,
};
use crate::LaunchPlan;

// The Rust test harness acts as a safe, cross-platform stand-in for Java.
#[test]
fn child_fixture() {
    if !std::env::args().any(|arg| arg == "--exact") {
        return;
    }
    println!("game stdout s3cr3t token");
    eprintln!("game stderr s3cr3t token");
    std::thread::sleep(Duration::from_millis(350));
}

fn test_plan() -> LaunchPlan {
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
            "process::tests::child_fixture".to_owned(),
            "--nocapture".to_owned(),
        ],
    }
}

#[test]
fn captures_both_streams_redacts_secrets_and_records_exit() {
    let plan = test_plan();
    let mut process = GameProcess::spawn(&plan, &["s3cr3t".to_owned()]).unwrap();
    assert!(process.id() > 0);
    let status = process.wait().unwrap();
    assert!(status.success());
    assert_eq!(process.try_wait().unwrap(), Some(status));

    let logs = process.drain_logs();
    assert!(logs.iter().any(|entry| {
        entry.stream == LogStream::Stdout && entry.line.contains("game stdout [REDACTED]")
    }), "expected redacted stdout log");
    assert!(logs.iter().any(|entry| {
        entry.stream == LogStream::Stderr && entry.line.contains("game stderr [REDACTED]")
    }), "expected redacted stderr log");
    assert!(logs.iter().all(|entry| !entry.line.contains("s3cr3t")));
}

#[test]
fn stop_terminates_a_running_child() {
    let plan = test_plan();
    let mut process = GameProcess::spawn(&plan, &[]).unwrap();
    let status = process.stop().unwrap();
    assert!(!status.success());
    assert_eq!(process.try_wait().unwrap(), Some(status));
    assert_eq!(process.stop().unwrap(), status);
}

#[test]
fn rejects_relative_executable_and_nonexistent_working_directory() {
    let mut plan = test_plan();
    plan.runtime.executable = PathBuf::from("java");
    assert!(matches!(
        GameProcess::spawn(&plan, &[]),
        Err(ProcessError::InvalidExecutable)
    ));
    plan = test_plan();
    plan.working_directory.push("no-such-directory-for-aether-test");
    assert!(matches!(
        GameProcess::spawn(&plan, &[]),
        Err(ProcessError::InvalidWorkingDirectory)
    ));
}

#[test]
fn truncates_oversized_lines_without_blocking_the_reader() {
    let mut bytes = vec![b'x'; MAX_LOG_BYTES + 200];
    bytes.push(b'\n');
    bytes.extend_from_slice(b"next line\n");

    let (sender, receiver) = mpsc::sync_channel(LOG_QUEUE_CAPACITY);
    let dropped = Arc::new(AtomicU64::new(0));
    let handle = spawn_reader(
        Cursor::new(bytes),
        LogStream::Stdout,
        sender,
        Arc::clone(&dropped),
        vec![],
    );
    handle.join().unwrap();
    let logs: Vec<_> = receiver.try_iter().collect();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].line.len(), MAX_LOG_BYTES);
    assert!(logs[0].truncated);
    assert_eq!(logs[1].line, "next line");
    assert!(!logs[1].truncated);
}

#[test]
fn drops_logs_instead_of_stalling_full_pipes() {
    let data = "line\n".repeat(300);
    let (sender, receiver) = mpsc::sync_channel(1);
    let dropped = Arc::new(AtomicU64::new(0));
    let thread = spawn_reader(
        Cursor::new(data.into_bytes()),
        LogStream::Stderr,
        sender,
        Arc::clone(&dropped),
        vec![],
    );
    thread.join().unwrap();
    assert_eq!(receiver.try_iter().count(), 1);
    assert_eq!(dropped.load(Ordering::Relaxed), 299);
}

#[test]
fn masks_common_token_flag_values_without_logging_argv() {
    let argv = vec![
        "--accessToken".to_owned(),
        "secret-long-token".to_owned(),
        "--username".to_owned(),
        "Alex".to_owned(),
    ];
    let masks = collect_redactions(&argv, &["another-secret".to_owned()]);
    assert!(masks.contains(&"secret-long-token".to_owned()));
    assert!(masks.contains(&"another-secret".to_owned()));
    assert!(!masks.contains(&"Alex".to_owned()));
}
