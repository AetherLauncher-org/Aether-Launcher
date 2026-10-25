use super::*;
use crate::evidence::{LaunchEvidence, PostExitSnapshot, PreLaunchSnapshot};

fn evidence(exit_code: Option<i32>, duration_millis: u64, output: &str) -> LaunchEvidence {
    LaunchEvidence {
        exit_code,
        duration_millis,
        stderr: output.to_string(),
        ..LaunchEvidence::default()
    }
}

#[test]
fn new_crash_report_means_game_crash() {
    let evidence = LaunchEvidence {
        exit_code: Some(1),
        duration_millis: 90_000,
        crash_reports: vec!["crash-2025-01-02_03.04.05-client.txt".to_string()],
        ..LaunchEvidence::default()
    };
    let verdict = classify_phase(&evidence);
    assert_eq!(verdict.phase, FailurePhase::GameCrash);
    assert!(verdict.summary().contains("崩溃报告"));
}

#[test]
fn crash_report_wins_over_quick_exit() {
    // A crash while the window is created can leave a report within seconds;
    // the report is the more specific evidence.
    let summary = SessionSummary {
        exit_code: Some(255),
        duration_millis: 900,
        output_bytes: 100,
        latest_log_grew: true,
        crash_report_count: 1,
        hs_err_count: 0,
    };
    assert_eq!(decide_phase(&summary).phase, FailurePhase::GameCrash);
}

#[test]
fn hs_err_means_native_crash() {
    let evidence = LaunchEvidence {
        exit_code: Some(134),
        duration_millis: 40_000,
        latest_log_delta: "some log".to_string(),
        hs_err_logs: vec!["hs_err_pid4242.log".to_string()],
        ..LaunchEvidence::default()
    };
    let verdict = classify_phase(&evidence);
    assert_eq!(verdict.phase, FailurePhase::NativeCrash);
    assert!(verdict.summary().contains("hs_err"));
}

#[test]
fn crash_report_wins_over_hs_err() {
    let summary = SessionSummary {
        exit_code: Some(134),
        duration_millis: 40_000,
        output_bytes: 20_000,
        latest_log_grew: true,
        crash_report_count: 2,
        hs_err_count: 1,
    };
    assert_eq!(decide_phase(&summary).phase, FailurePhase::GameCrash);
}

#[test]
fn quick_exit_with_short_output_is_jvm_startup_failure() {
    let mut evidence = evidence(
        Some(1),
        1500,
        "Unrecognized option: --sun-misc-unsafe-memory-access=allow\nCould not create the Java Virtual Machine.\n",
    );
    evidence.pre.latest_log_len = Some(0);
    evidence.post.latest_log_len = Some(0);
    let verdict = classify_phase(&evidence);
    assert_eq!(verdict.phase, FailurePhase::JvmStartupFailed);
    assert!(!verdict.reasons.is_empty());
}

#[test]
fn quick_exit_without_log_growth_counts_as_startup_failure() {
    let summary = SessionSummary {
        exit_code: Some(1),
        duration_millis: 300,
        output_bytes: 50,
        latest_log_grew: false,
        crash_report_count: 0,
        hs_err_count: 0,
    };
    assert_eq!(decide_phase(&summary).phase, FailurePhase::JvmStartupFailed);
}

#[test]
fn long_run_with_large_output_is_not_a_startup_failure() {
    let summary = SessionSummary {
        exit_code: Some(1),
        duration_millis: 120_000,
        output_bytes: 200_000,
        latest_log_grew: true,
        crash_report_count: 0,
        hs_err_count: 0,
    };
    assert_eq!(decide_phase(&summary).phase, FailurePhase::LoaderFailure);
}

#[test]
fn slow_exit_is_a_loader_failure_even_with_little_output() {
    // The JVM took its time; that is past "the JVM never started".
    let summary = SessionSummary {
        exit_code: Some(2),
        duration_millis: 45_000,
        output_bytes: 300,
        latest_log_grew: false,
        crash_report_count: 0,
        hs_err_count: 0,
    };
    assert_eq!(decide_phase(&summary).phase, FailurePhase::LoaderFailure);
}

#[test]
fn loud_startup_failure_is_a_loader_failure() {
    // Fast exit but a lot of output: the loader printed something and died,
    // which is exactly the loader-failure row of the table.
    let summary = SessionSummary {
        exit_code: Some(1),
        duration_millis: 3_000,
        output_bytes: SHORT_OUTPUT_BYTES,
        latest_log_grew: true,
        crash_report_count: 0,
        hs_err_count: 0,
    };
    assert_eq!(decide_phase(&summary).phase, FailurePhase::LoaderFailure);
}

#[test]
fn exit_code_without_any_output_is_no_output() {
    let summary = SessionSummary {
        exit_code: Some(1),
        duration_millis: 800,
        output_bytes: 0,
        latest_log_grew: false,
        crash_report_count: 0,
        hs_err_count: 0,
    };
    let verdict = decide_phase(&summary);
    assert_eq!(verdict.phase, FailurePhase::NoOutput);
    assert!(verdict.summary().contains("退出码 1"));
}

#[test]
fn missing_exit_code_without_output_is_no_output() {
    let summary = SessionSummary {
        exit_code: None,
        duration_millis: 800,
        output_bytes: 0,
        latest_log_grew: false,
        crash_report_count: 0,
        hs_err_count: 0,
    };
    let verdict = decide_phase(&summary);
    assert_eq!(verdict.phase, FailurePhase::NoOutput);
    assert!(verdict.summary().contains("没有留下退出码"));
}

#[test]
fn clean_exit_is_not_a_failure() {
    let summary = SessionSummary {
        exit_code: Some(0),
        duration_millis: 60_000,
        output_bytes: 100_000,
        latest_log_grew: true,
        crash_report_count: 0,
        hs_err_count: 0,
    };
    let verdict = decide_phase(&summary);
    assert_eq!(verdict.phase, FailurePhase::Unknown);
    assert!(!verdict.phase.is_conclusive());
}

#[test]
fn empty_log_delta_still_reads_growth_from_snapshots() {
    let evidence = LaunchEvidence {
        exit_code: Some(1),
        duration_millis: 30_000,
        pre: PreLaunchSnapshot {
            latest_log_len: Some(10),
            ..PreLaunchSnapshot::default()
        },
        post: PostExitSnapshot {
            latest_log_len: Some(20),
            ..PostExitSnapshot::default()
        },
        ..LaunchEvidence::default()
    };
    let summary = SessionSummary::from_evidence(&evidence);
    assert!(summary.latest_log_grew);
    assert_eq!(summary.output_bytes, 0);
    // A log grew, so this is not "no output at all".
    assert_eq!(decide_phase(&summary).phase, FailurePhase::LoaderFailure);
}

#[test]
fn phase_labels_are_chinese_and_stable() {
    assert_eq!(FailurePhase::JvmStartupFailed.label_zh(), "Java 启动失败");
    assert_eq!(FailurePhase::GameCrash.as_str(), "game_crash");
    assert_eq!(FailurePhase::NativeCrash.as_str(), "native_crash");
    assert_eq!(FailurePhase::LoaderFailure.as_str(), "loader_failure");
    assert_eq!(FailurePhase::NoOutput.as_str(), "no_output");
    assert_eq!(FailurePhase::Unknown.as_str(), "unknown");
    assert!(FailurePhase::NoOutput.is_conclusive());
}

#[test]
fn reasons_never_repeat_and_are_ordered() {
    let summary = SessionSummary {
        exit_code: Some(7),
        duration_millis: 10,
        output_bytes: 10,
        latest_log_grew: false,
        crash_report_count: 0,
        hs_err_count: 0,
    };
    let verdict = decide_phase(&summary);
    assert_eq!(verdict.phase, FailurePhase::JvmStartupFailed);
    assert_eq!(verdict.reasons.len(), 2);
    assert!(verdict.reasons[0].contains("10 毫秒") || verdict.reasons[0].contains("0.0 秒"));
}
