use super::*;

fn one(name: &str) -> Vec<String> {
    vec![name.to_string()]
}

#[test]
fn crash_report_names_follow_minecraft_pattern() {
    assert!(is_crash_report_name("crash-2025-01-02_03.04.05-client.txt"));
    assert!(is_crash_report_name("crash-1999-12-31_23.59.59-server.txt"));
    assert!(!is_crash_report_name("crash-2025-01-02_03.04.05.txt"));
    assert!(!is_crash_report_name(
        "crash-2025-01-02_03.04.05-client.log"
    ));
    assert!(!is_crash_report_name("crash-report.txt"));
    assert!(!is_crash_report_name("latest.log"));
    assert!(!is_crash_report_name(""));
    assert!(!is_crash_report_name("crash-2025-01-0203.04.05-client.txt"));
}

#[test]
fn hs_err_names_follow_jvm_pattern() {
    assert!(is_hs_err_name("hs_err_pid1234.log"));
    assert!(is_hs_err_name("hs_err_pid1.log"));
    assert!(!is_hs_err_name("hs_err_pid.log"));
    assert!(!is_hs_err_name("hs_err_pid12.txt"));
    assert!(!is_hs_err_name("hs_err_pid12ab.log"));
    assert!(!is_hs_err_name("replay_pid12.log"));
}

#[test]
fn appended_tail_is_the_new_part() {
    let full = "old line\nnew line one\nnew line two\n";
    let cut = new_since_known_len(9, full);
    assert_eq!(cut, "new line one\nnew line two\n");
}

#[test]
fn identical_length_yields_nothing() {
    let full = "same content";
    assert_eq!(new_since_known_len(full.len() as u64, full), "");
}

#[test]
fn truncated_log_counts_as_fully_new() {
    // The launcher recorded more bytes than the file now has: log4j rotated
    // the file, so the old content is gone and everything we see is new.
    let full = "rotated\n";
    assert_eq!(new_since_known_len(4096, full), full);
}

#[test]
fn cut_inside_multibyte_character_moves_to_next_boundary() {
    // Each CJK character below is three bytes in UTF-8; cutting at offset 1
    // lands inside the first one.
    let full = "a\u{5d29}\u{6e83}";
    let cut = new_since_known_len(1, full);
    assert_eq!(cut, "\u{5d29}\u{6e83}");
}

#[test]
fn pathological_known_len_does_not_panic() {
    let full = "abc";
    assert_eq!(new_since_known_len(u64::MAX, full), full);
    assert_eq!(new_since_known_len(0, full), full);
}

#[test]
fn delta_kind_reports_rotation() {
    assert_eq!(delta_kind(None, None), LogDeltaKind::Missing);
    assert_eq!(delta_kind(None, Some("")), LogDeltaKind::Missing);
    assert_eq!(delta_kind(None, Some("hi")), LogDeltaKind::Fresh);
    assert_eq!(delta_kind(Some(4), Some("hihi")), LogDeltaKind::Unchanged);
    assert_eq!(delta_kind(Some(4), Some("hi")), LogDeltaKind::Rotated);
    assert_eq!(delta_kind(Some(2), Some("hi")), LogDeltaKind::Unchanged);
    assert!(LogDeltaKind::Rotated.has_new_content());
    assert!(!LogDeltaKind::Unchanged.has_new_content());
    assert!(!LogDeltaKind::Missing.has_new_content());
}

#[test]
fn session_detects_growth_and_new_files() {
    let pre = PreLaunchSnapshot {
        latest_log_len: Some(100),
        crash_reports_newest: Some(10),
        hs_err_newest: Some(10),
        logs_newest: Some(10),
    };
    let post = PostExitSnapshot {
        latest_log_len: Some(900),
        crash_reports_newest: Some(10),
        hs_err_newest: Some(10),
        logs_newest: Some(900),
    };
    let session = EvidenceSession::new(pre, post);
    assert!(session.latest_log_grew);
    assert!(!session.crash_report_appeared);
    assert!(!session.hs_err_appeared);
    assert!(session.new_files().is_empty());
}

#[test]
fn session_flags_newer_timestamp_as_appearance() {
    let pre = PreLaunchSnapshot {
        crash_reports_newest: Some(10),
        hs_err_newest: None,
        ..PreLaunchSnapshot::default()
    };
    let post = PostExitSnapshot {
        crash_reports_newest: Some(11),
        hs_err_newest: Some(999),
        ..PostExitSnapshot::default()
    };
    let session = EvidenceSession::new(pre, post);
    assert!(session.crash_report_appeared);
    assert!(session.hs_err_appeared);
}

#[test]
fn unchanged_timestamp_is_not_an_appearance() {
    let pre = PreLaunchSnapshot {
        crash_reports_newest: Some(10),
        ..PreLaunchSnapshot::default()
    };
    let post = PostExitSnapshot {
        crash_reports_newest: Some(10),
        ..PostExitSnapshot::default()
    };
    assert!(!EvidenceSession::new(pre, post).crash_report_appeared);
}

#[test]
fn session_drops_unrecognised_file_names() {
    let session = EvidenceSession::from_snapshots(
        PreLaunchSnapshot::default(),
        PostExitSnapshot::default(),
        &[
            "crash-2025-01-02_03.04.05-client.txt".to_string(),
            "notes.txt".to_string(),
        ],
        &[
            "hs_err_pid42.log".to_string(),
            "hs_err_pidX.log".to_string(),
        ],
    );
    assert_eq!(
        session.crash_reports(),
        ["crash-2025-01-02_03.04.05-client.txt"]
    );
    assert_eq!(session.hs_err_logs(), ["hs_err_pid42.log"]);
    assert!(session.crash_report_appeared);
    assert!(session.hs_err_appeared);
    assert_eq!(
        session.new_files(),
        [
            "crash-2025-01-02_03.04.05-client.txt".to_string(),
            "hs_err_pid42.log".to_string()
        ]
    );
}

#[test]
fn session_with_files_accepts_a_single_entry() {
    let session = EvidenceSession::new(PreLaunchSnapshot::default(), PostExitSnapshot::default())
        .with_files(&one("crash-2020-02-02_02.02.02-client.txt"), &[]);
    assert_eq!(session.crash_reports().len(), 1);
    assert!(session.hs_err_logs().is_empty());
}

#[test]
fn session_into_evidence_preserves_process_facts() {
    let session = EvidenceSession::new(PreLaunchSnapshot::default(), PostExitSnapshot::default());
    let evidence = session.into_evidence(
        vec!["-Xmx2G".to_string()],
        Some(1),
        1234,
        CapturedText {
            latest_log_delta: "log".to_string(),
            latest_log_rotated: true,
            stdout: "out".to_string(),
            stderr: "err".to_string(),
            system_info: Some("java.version=21.0.1".to_string()),
            java_major: Some(21),
        },
    );
    assert_eq!(evidence.exit_code, Some(1));
    assert_eq!(evidence.duration_millis, 1234);
    assert_eq!(evidence.java_major, Some(21));
    assert!(evidence.latest_log_rotated);
    assert!(evidence.has_any_output());
    assert_eq!(evidence.output_len(), 9);
}

#[test]
fn combined_text_orders_log_then_stderr_then_stdout() {
    let evidence = LaunchEvidence {
        latest_log_delta: "log".to_string(),
        stderr: "err".to_string(),
        stdout: "out".to_string(),
        ..LaunchEvidence::default()
    };
    assert_eq!(evidence.combined_text(), "log\nerr\nout");
}

#[test]
fn combined_text_skips_empty_blocks() {
    let evidence = LaunchEvidence {
        stdout: "out".to_string(),
        ..LaunchEvidence::default()
    };
    assert_eq!(evidence.combined_text(), "out");
    assert!(!LaunchEvidence::default().has_any_output());
}

#[test]
fn tail_never_splits_a_character() {
    // Two bytes per Greek letter: asking for five bytes may not split one.
    let text = "\u{3b1}\u{3b2}\u{3b3}\u{3b4}";
    assert_eq!(tail_at_most(text, 5), "\u{3b3}\u{3b4}");
    assert_eq!(tail_at_most(text, 100), text);
    assert_eq!(tail_at_most(text, 2), "\u{3b4}");
}

#[test]
fn evidence_directory_names_are_stable() {
    assert_eq!(EvidenceDirectory::Logs.as_str(), "logs");
    assert_eq!(EvidenceDirectory::CrashReports.as_str(), "crash-reports");
    assert_eq!(EvidenceDirectory::InstanceRoot.as_str(), "instance-root");
}
