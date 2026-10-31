use super::*;

const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

#[test]
fn only_lower_case_sha1_is_accepted() {
    assert!(is_sha1(HASH));
    assert!(!is_sha1(&HASH.to_uppercase()));
    assert!(!is_sha1(&HASH[..39]));
    assert!(!is_sha1(&format!("{HASH}0")));
    assert!(!is_sha1(&"g".repeat(40)));
    assert!(!is_sha1(""));
}

#[test]
fn the_lookup_body_names_the_hashes_and_the_algorithm() {
    let body = version_files_body(&[HASH.to_owned()]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["algorithm"], "sha1");
    assert_eq!(value["hashes"][0], HASH);
}

#[test]
fn the_lookup_body_refuses_bad_hashes_and_too_many() {
    assert_eq!(
        version_files_body(&["../x".to_owned()]),
        Err(ContentError::InvalidHash)
    );
    let many = vec![HASH.to_owned(); MAX_PER_REQUEST + 1];
    assert_eq!(
        version_files_body(&many),
        Err(ContentError::TooMany(MAX_PER_REQUEST))
    );
    assert!(version_files_body(&[]).is_ok());
}

#[test]
fn versions_are_read_by_hash() {
    let body = format!(
        r#"{{"{HASH}":{{"project_id":"AANobbMI","version_number":"0.6.3","name":"Sodium 0.6.3","files":[],"extra":{{"deep":[1,2]}}}}}}"#
    );
    let versions = parse_version_files(&body).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[HASH].project_id, "AANobbMI");
    assert_eq!(versions[HASH].version, "0.6.3");
}

#[test]
fn a_version_without_a_number_falls_back_to_its_name() {
    let body = format!(r#"{{"{HASH}":{{"project_id":"AANobbMI","name":"Release 2"}}}}"#);
    assert_eq!(parse_version_files(&body).unwrap()[HASH].version, "Release 2");
}

#[test]
fn entries_with_a_bad_key_or_project_id_are_dropped() {
    let body = format!(
        r#"{{"not-a-hash":{{"project_id":"AANobbMI"}},"{HASH}":{{"project_id":"../../x"}}}}"#
    );
    assert!(parse_version_files(&body).unwrap().is_empty());
}

#[test]
fn a_wrong_shape_or_an_oversized_answer_is_refused() {
    assert_eq!(parse_version_files("[]"), Err(ContentError::Malformed));
    assert_eq!(parse_version_files("nope"), Err(ContentError::Malformed));
    assert_eq!(
        parse_version_files(&format!(r#"{{"{HASH}":{{"project_id":5}}}}"#)),
        Err(ContentError::Malformed)
    );
    let huge = format!("{{\"a\":\"{}\"}}", "x".repeat(MAX_BODY_BYTES));
    assert_eq!(
        parse_version_files(&huge),
        Err(ContentError::TooLarge(MAX_BODY_BYTES))
    );
}

#[test]
fn deeply_nested_answers_do_not_overflow_the_stack() {
    let nested = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert_eq!(parse_version_files(&nested), Err(ContentError::Malformed));
    assert_eq!(parse_projects(&nested), Err(ContentError::Malformed));
}

#[test]
fn the_projects_url_encodes_a_json_list() {
    let url = projects_url(&["AANobbMI".to_owned(), "gvQqBUqZ".to_owned()]).unwrap();
    assert_eq!(
        url,
        "https://api.modrinth.com/v2/projects?ids=%5B%22AANobbMI%22%2C%22gvQqBUqZ%22%5D"
    );
}

#[test]
fn the_projects_url_refuses_ids_that_could_change_the_request() {
    for bad in ["", "a b", "a\",\"b", "a&x=1", "../x", "a/b"] {
        assert_eq!(
            projects_url(&[bad.to_owned()]),
            Err(ContentError::InvalidId),
            "{bad:?}"
        );
    }
    let many = vec!["AANobbMI".to_owned(); MAX_PER_REQUEST + 1];
    assert_eq!(
        projects_url(&many),
        Err(ContentError::TooMany(MAX_PER_REQUEST))
    );
}

#[test]
fn icons_must_be_on_the_platform_cdn() {
    assert!(trusted_icon_url("https://cdn.modrinth.com/data/AANobbMI/icon.png").is_some());
    for bad in [
        "http://cdn.modrinth.com/data/x/icon.png",
        "https://cdn.modrinth.com.evil.test/x.png",
        "https://evil.test/cdn.modrinth.com/x.png",
        "https://cdn.modrinth.com@evil.test/x.png",
        "https://cdn.modrinth.com:8443/x.png",
        "https://cdn.modrinth.com/",
        "https://cdn.modrinth.com",
        "https://cdn.modrinth.com/a b.png",
        "https://cdn.modrinth.com/a\\b.png",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "data:image/png;base64,AAAA",
        "",
    ] {
        assert_eq!(trusted_icon_url(bad), None, "{bad:?}");
    }
    let long = format!("https://cdn.modrinth.com/{}", "a".repeat(600));
    assert_eq!(trusted_icon_url(&long), None);
}

#[test]
fn projects_are_read_and_cleaned() {
    let body = r#"[
        {"id":"AANobbMI","slug":"sodium","title":"Sodium\n","description":"A modern\nrendering engine","project_type":"mod","icon_url":"https://cdn.modrinth.com/data/AANobbMI/icon.png","body":"ignored"},
        {"id":"gvQqBUqZ","slug":null,"title":"Lithium","description":null,"project_type":"mod","icon_url":"https://evil.test/x.png"}
    ]"#;
    let projects = parse_projects(body).unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].title, "Sodium");
    assert_eq!(projects[0].summary, "A modern rendering engine");
    assert_eq!(projects[0].slug, "sodium");
    assert!(projects[0].icon_url.is_some());
    assert_eq!(projects[1].slug, "gvQqBUqZ");
    assert_eq!(projects[1].summary, "");
    assert_eq!(projects[1].icon_url, None);
}

#[test]
fn projects_of_other_types_or_without_a_title_are_dropped() {
    let body = r#"[
        {"id":"AANobbMI","slug":"a","title":"A","project_type":"plugin"},
        {"id":"gvQqBUqZ","slug":"b","title":"  \n ","project_type":"mod"},
        {"id":"../x","slug":"c","title":"C","project_type":"mod"},
        {"id":"Zz9","slug":"d","title":"D","project_type":"shader"}
    ]"#;
    let projects = parse_projects(body).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, "Zz9");
}

#[test]
fn an_odd_slug_is_replaced_by_the_id_so_the_page_url_stays_safe() {
    let body = r#"[{"id":"AANobbMI","slug":"a/../b","title":"A","project_type":"mod"}]"#;
    assert_eq!(parse_projects(body).unwrap()[0].slug, "AANobbMI");
}

#[test]
fn the_page_url_is_only_built_from_a_slug() {
    assert_eq!(
        project_page_url("sodium").as_deref(),
        Some("https://modrinth.com/project/sodium")
    );
    for bad in ["", "a/b", "a?b", "a#b", "a b", "..", ".hidden", "http://x"] {
        assert_eq!(project_page_url(bad), None, "{bad:?}");
    }
}
