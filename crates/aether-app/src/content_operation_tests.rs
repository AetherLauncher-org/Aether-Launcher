use super::*;
use aether_core::InstanceId;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

fn completed(id: &str) -> ContentInstallResult {
    ContentInstallResult {
        operation_id: id.to_owned(),
        status: "done",
        installed: vec![InstalledFile {
            project_id: "AAAAAAAA".to_owned(),
            file_name: "main.jar".to_owned(),
            version: "1.0".to_owned(),
        }],
        skipped: vec![SkippedProject {
            project_id: "BBBBBBBB".to_owned(),
            reason: "no compatible version".to_owned(),
        }],
        error: None,
    }
}

#[test]
fn reservations_have_unique_uuid_v4_ids_and_enforce_two_active_slots() {
    let state = ContentInstallState::default();
    let first = state.begin().unwrap();
    let second = state.begin().unwrap();
    assert_ne!(first, second);
    for id in [&first, &second] {
        assert!(aether_core::InstanceId::parse(id).is_ok());
        assert_eq!(id.as_bytes()[14], b'4');
    }
    assert!(state.begin().is_err());
    state.start(&first).unwrap();
    assert!(state.begin().is_err());
    state.finish(completed(&first));
    assert!(state.begin().is_ok());
    assert!(state.begin().is_err());
}

#[test]
fn cancellation_is_scoped_to_one_operation_and_survives_start() {
    let state = ContentInstallState::default();
    let first = state.begin().unwrap();
    let second = state.begin().unwrap();
    assert!(!state.cancel("unknown"));
    assert!(state.cancel(&first));
    assert!(state.cancel(&first));
    let first_control = state.start(&first).unwrap();
    let second_control = state.start(&second).unwrap();
    assert!(first_control.0.cancelled.load(Ordering::Acquire));
    assert!(!second_control.0.cancelled.load(Ordering::Acquire));
    // Requesting cancellation alone must not free a still-running slot.
    assert!(state.begin().is_err());
    state.finish(completed(&first));
    assert!(!state.cancel(&first));
    assert!(state.cancel(&second));
    assert!(second_control.0.cancelled.load(Ordering::Acquire));
}

#[test]
fn terminal_results_remain_available_to_repeated_waiters() {
    let state = ContentInstallState::default();
    let id = state.begin().unwrap();
    assert!(state.result(&id).is_none());
    let expected = completed(&id);
    assert_eq!(state.finish(expected.clone()), expected);
    assert_eq!(state.result(&id), Some(expected.clone()));
    assert_eq!(state.wait(&id).unwrap(), expected);
    assert_eq!(state.wait(&id).unwrap(), expected);
    assert!(state.wait("unknown").is_err());
    assert!(state.result("unknown").is_none());
}

#[test]
fn the_first_terminal_result_wins_without_duplicate_retention_entries() {
    let state = ContentInstallState::default();
    let id = state.begin().unwrap();
    let expected = completed(&id);
    state.finish(expected.clone());
    let mut conflicting = expected.clone();
    conflicting.status = "failed";
    conflicting.error = Some("late failure".to_owned());
    conflicting.installed.clear();
    assert_eq!(state.finish(conflicting), expected);
    assert_eq!(
        state.fail(&id, "late worker failure".to_owned()).unwrap(),
        expected
    );
    assert_eq!(state.wait(&id).unwrap(), expected);
    let registry = state.registry.lock().unwrap();
    assert_eq!(registry.finished.len(), 1);
    assert_eq!(registry.entries.len(), 1);
}

#[test]
fn fail_completes_unstarted_reservations_but_refuses_running_workers() {
    let state = ContentInstallState::default();
    let reserved = state.begin().unwrap();
    let running = state.begin().unwrap();
    state.start(&running).unwrap();
    assert!(state.start(&running).is_err());
    assert!(
        state
            .fail(&running, "cannot overwrite worker".to_owned())
            .is_err()
    );
    assert!(state.result(&running).is_none());
    let result = state
        .fail(&reserved, "could not launch worker".to_owned())
        .unwrap();
    assert_eq!(result.operation_id, reserved);
    assert_eq!(result.status, "failed");
    assert_eq!(result.error.as_deref(), Some("could not launch worker"));
    assert!(result.installed.is_empty());
    assert!(result.skipped.is_empty());
    assert_eq!(state.wait(&reserved).unwrap(), result);
    assert!(state.start(&reserved).is_err());
    assert!(state.fail("unknown", "missing".to_owned()).is_err());
    assert!(state.start("unknown").is_err());
    assert!(state.begin().is_ok());
}

#[test]
fn retaining_the_latest_128_results_does_not_evict_active_operations() {
    let state = ContentInstallState::default();
    let active = state.begin().unwrap();
    let mut ids = Vec::new();
    for _ in 0..129 {
        let id = state.begin().unwrap();
        state.finish(completed(&id));
        ids.push(id);
    }
    assert!(state.result(&ids[0]).is_none());
    assert!(state.wait(&ids[0]).is_err());
    assert!(!state.cancel(&ids[0]));
    for id in &ids[1..] {
        assert_eq!(state.wait(id).unwrap(), completed(id));
    }
    assert!(state.cancel(&active));
    assert!(state.start(&active).is_ok());
    let registry = state.registry.lock().unwrap();
    assert_eq!(registry.finished.len(), 128);
    assert_eq!(registry.entries.len(), 129);
    assert!(registry.entries.contains_key(&active));
}

#[test]
fn finishing_on_another_thread_wakes_a_waiter_with_the_exact_report() {
    let state = Arc::new(ContentInstallState::default());
    let id = state.begin().unwrap();
    let expected = completed(&id);
    let (send, receive) = mpsc::channel();
    let waiting_state = Arc::clone(&state);
    let waiting_id = id.clone();
    let waiter = thread::spawn(move || {
        send.send(waiting_state.wait(&waiting_id)).unwrap();
    });
    state.finish(expected.clone());
    let received = receive.recv_timeout(Duration::from_secs(5));
    waiter.join().unwrap();
    assert_eq!(received.unwrap().unwrap(), expected);
}

#[test]
fn an_attached_waiter_keeps_its_result_after_registry_eviction() {
    let state = Arc::new(ContentInstallState::default());
    let id = state.begin().unwrap();
    // Hold the same Arc a waiter acquires before waiting, so registry eviction cannot
    // destroy its condition variable or terminal report.
    let operation = state.operation(&id).unwrap();
    let (attached_send, attached_receive) = mpsc::channel();
    let (result_send, result_receive) = mpsc::channel();
    let waiter = thread::spawn(move || {
        let mut data = operation.data.lock().unwrap();
        attached_send.send(()).unwrap();
        while data.result.is_none() {
            data = operation.ready.wait(data).unwrap();
        }
        result_send.send(data.result.clone().unwrap()).unwrap();
    });
    attached_receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let expected = completed(&id);
    state.finish(expected.clone());
    for _ in 0..128 {
        let next = state.begin().unwrap();
        state.finish(completed(&next));
    }
    assert!(state.result(&id).is_none());
    assert!(state.wait(&id).is_err());
    let received = result_receive.recv_timeout(Duration::from_secs(5));
    waiter.join().unwrap();
    assert_eq!(received.unwrap(), expected);
}

/// A host that only records what the control layer emits.
#[derive(Default)]
struct RecordingHost {
    events: Mutex<Vec<ContentInstallProgress>>,
}

impl RecordingHost {
    fn snapshot(&self) -> Vec<ContentInstallProgress> {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl Host for RecordingHost {
    fn emit_content_install(&self, progress: &ContentInstallProgress) {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(progress.clone());
    }
    fn data_dir(&self) -> Result<PathBuf, String> {
        Err("not used".to_owned())
    }
    fn is_instance_running(&self, _id: InstanceId) -> Result<bool, String> {
        Ok(false)
    }
    fn emit_login(&self, _event: &crate::microsoft::LoginEvent) {}
    fn emit_install(&self, _progress: &crate::install::InstallProgress) {}
    fn emit_import_item(&self, _event: &crate::import::ImportItemEvent) {}
    fn pick_folder(&self, _title: &str) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
    fn pick_file(
        &self,
        _filter_name: &str,
        _extensions: &[&str],
    ) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
    fn known_folders(&self) -> crate::host::KnownFolders {
        crate::host::KnownFolders {
            desktop: None,
            downloads: None,
            documents: None,
            home: None,
        }
    }
}

#[test]
fn throttling_sends_the_first_update_then_waits_for_the_interval() {
    fn empty_report() -> InstallReport {
        InstallReport {
            installed: Vec::new(),
            skipped: Vec::new(),
        }
    }
    fn interval_closed_forever(control: &OperationControl) {
        // a future stamp saturates `duration_since` to zero, so the window can never
        // elapse however the scheduler stalls this test
        control.progress.lock().unwrap().1 = Some(Instant::now() + Duration::from_secs(3600));
    }
    let host = RecordingHost::default();
    let state = ContentInstallState::default();
    let id = state.begin().unwrap();
    let control = OperationControl::new(&host, &id, "AAAAAAAA", state.start(&id).unwrap());
    control.resolving("AAAAAAAA");
    interval_closed_forever(&control);
    control.downloading("main.jar", 128 * 1024);
    control.progress(64 * 1024, 128 * 1024);
    control.progress(128 * 1024, 128 * 1024);
    let snapshot = host.snapshot();
    // the first update is always sent; updates inside the interval are coalesced
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].phase, "resolving");
    // the swallowed updates still move the state the terminal event carries
    control.terminal("done", &empty_report());
    let snapshot = host.snapshot();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot[1].phase, "done");
    assert_eq!(snapshot[1].file_name.as_deref(), Some("main.jar"));
    assert_eq!(snapshot[1].bytes_done, 128 * 1024);
    assert_eq!(snapshot[1].bytes_total, 128 * 1024);
    assert_eq!(snapshot[1].files_done, 0);
}

#[test]
fn a_controlled_lifecycle_emits_monotonic_progress_ending_in_a_terminal_phase() {
    fn interval_already_open(control: &OperationControl) {
        // a stamp a second in the past can never fall back inside the 100 ms window,
        // so every update emits however the scheduler stalls this test
        control.progress.lock().unwrap().1 = Some(
            Instant::now()
                .checked_sub(Duration::from_secs(1))
                .expect("the monotonic clock is at least a second old"),
        );
    }
    let host = Arc::new(RecordingHost::default());
    let state = ContentInstallState::default();
    let id = state.begin().unwrap();
    let control = OperationControl::new(&*host, &id, "AAAAAAAA", state.start(&id).unwrap());
    control.resolving("AAAAAAAA");
    interval_already_open(&control);
    control.downloading("main.jar", 128 * 1024);
    for done in [32 * 1024_u64, 64 * 1024, 96 * 1024, 128 * 1024] {
        interval_already_open(&control);
        control.progress(done, 128 * 1024);
    }
    interval_already_open(&control);
    control.verifying();
    interval_already_open(&control);
    control.committed();
    let report = InstallReport {
        installed: vec![InstalledFile {
            project_id: "AAAAAAAA".to_owned(),
            file_name: "main.jar".to_owned(),
            version: "1.0".to_owned(),
        }],
        skipped: Vec::new(),
    };
    control.terminal("done", &report);
    let events = host.snapshot();
    let phases: Vec<&str> = events.iter().map(|event| event.phase).collect();
    assert_eq!(
        phases,
        [
            "resolving",
            "downloading",
            "downloading",
            "downloading",
            "downloading",
            "downloading",
            "verifying",
            "verifying",
            "done",
        ]
    );
    assert!(events.iter().all(|event| event.operation_id == id));
    assert!(
        events
            .iter()
            .all(|event| event.bytes_done <= event.bytes_total)
    );
    for pair in events.windows(2) {
        assert!(pair[1].bytes_done >= pair[0].bytes_done, "{events:?}");
    }
    let last = events.last().unwrap();
    assert_eq!(last.files_done, 1);
    assert!(!last.resolving);
    assert_eq!(last.file_name.as_deref(), Some("main.jar"));
}

#[test]
fn execute_completes_a_failing_operation_and_frees_its_slot() {
    let host = Arc::new(RecordingHost::default());
    let state = Arc::new(ContentInstallState::default());
    let id = state.begin().unwrap();
    let waiting_state = Arc::clone(&state);
    let waiting_id = id.clone();
    let waiter = thread::spawn(move || waiting_state.wait(&waiting_id));
    execute(
        &state,
        &id,
        &*host,
        "not-an-instance",
        "AAAAAAAA",
        "mod",
        None,
        false,
    );
    let result = waiter.join().unwrap().unwrap();
    assert_eq!(result.status, "failed");
    assert_eq!(result.operation_id, id);
    assert!(result.error.is_some());
    let events = host.snapshot();
    assert_eq!(events.last().map(|event| event.phase), Some("failed"));
    // The failure path must still count as finished so the slot frees.
    assert!(state.begin().is_ok());
}
