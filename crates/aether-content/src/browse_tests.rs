use super::*;

const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";
const SHA512: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\
                      0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn query<'a>() -> SearchQuery<'a> {
    SearchQuery {
        text: "sodium",
        project_type: ProjectType::Mod,
        game_version: Some("1.21.1"),
        loader: Some("fabric"),
        sort: Sort::Downloads,
        offset: 20,
        limit: 10,
    }
}

#[test]
fn the_search_address_carries_the_facets_percent_encoded() {
    let url = search_url(&query()).unwrap();

    assert!(url.starts_with("https://api.modrinth.com/v2/search?query=sodium&facets="));
    // [["project_type:mod"],["versions:1.21.1"],["categories:fabric"]]
    assert!(url.contains("%5B%5B%22project_type%3Amod%22%5D%2C%5B%22versions%3A1.21.1%22%5D%2C%5B%22categories%3Afabric%22%5D%5D"));
    assert!(url.ends_with("&index=downloads&offset=20&limit=10"));
}

#[test]
fn search_text_cannot_break_out_of_its_parameter() {
    let mut q = query();
    q.text = "a&index=newest#x y\n\"é";
    let url = search_url(&q).unwrap();

    let text = url.split("query=").nth(1).unwrap().split("&facets=").next().unwrap();
    assert_eq!(text, "a%26index%3Dnewest%23x%20y%20%22%C3%A9");
    assert_eq!(url.matches("&index=").count(), 1);
}

#[test]
fn only_real_game_versions_and_loaders_are_put_in_an_address() {
    for bad in ["", "1.21 1", "1.21\"]", "a/b", &"9".repeat(40)] {
        let mut q = query();
        q.game_version = Some(bad);
        assert_eq!(search_url(&q), Err(ContentError::InvalidId), "{bad:?}");
        assert_eq!(versions_url("AANobbMI", Some(bad), &[]), Err(ContentError::InvalidId));
    }
    let mut q = query();
    q.loader = Some("fabric\"]");
    assert_eq!(search_url(&q), Err(ContentError::InvalidId));
    assert_eq!(versions_url("AANobbMI", None, &["evil"]), Err(ContentError::InvalidId));
    assert_eq!(versions_url("../x", None, &[]), Err(ContentError::InvalidId));
}

#[test]
fn the_page_size_and_offset_are_bounded() {
    let mut q = query();
    q.limit = 5000;
    q.offset = u32::MAX;
    let url = search_url(&q).unwrap();
    assert!(url.ends_with("&offset=10000&limit=50"));
    q.limit = 0;
    assert!(search_url(&q).unwrap().ends_with("&limit=1"));
}

#[test]
fn the_versions_address_filters_by_loader_and_game() {
    assert_eq!(
        versions_url("AANobbMI", Some("1.21.1"), &["fabric", "quilt"]).unwrap(),
        "https://api.modrinth.com/v2/project/AANobbMI/version?loaders=%5B%22fabric%22%2C%22quilt%22%5D&game_versions=%5B%221.21.1%22%5D"
    );
    assert_eq!(
        versions_url("AANobbMI", None, &[]).unwrap(),
        "https://api.modrinth.com/v2/project/AANobbMI/version"
    );
}

#[test]
fn search_hits_are_cleaned_and_unknown_ones_dropped() {
    let body = r#"{"total_hits": 120, "hits": [
        {"project_id":"AANobbMI","slug":"sodium","title":"Sodium\n","description":"Fast\tmod","author":"jellysquid3",
         "downloads":99,"icon_url":"https://cdn.modrinth.com/data/AANobbMI/icon.png","project_type":"mod"},
        {"project_id":"bad id!","title":"x","project_type":"mod"},
        {"project_id":"AAAAAAAA","title":"x","project_type":"plugin"},
        {"project_id":"BBBBBBBB","title":"  ","project_type":"mod"},
        {"project_id":"CCCCCCCC","slug":"../evil","title":"Pack","project_type":"resourcepack",
         "icon_url":"https://evil.test/i.png"}
    ]}"#;

    let page = parse_search(body).unwrap();

    assert_eq!(page.total, 120);
    assert_eq!(page.hits.len(), 2);
    assert_eq!(page.hits[0].title, "Sodium");
    assert_eq!(page.hits[0].summary, "Fast mod");
    assert!(page.hits[0].icon_url.is_some());
    assert_eq!(page.hits[1].slug, "CCCCCCCC");
    assert_eq!(page.hits[1].icon_url, None);
}

#[test]
fn a_hit_carries_loaders_tags_versions_and_sides_apart() {
    let body = r#"{"total_hits": 1, "hits": [
        {"project_id":"AANobbMI","slug":"sodium","title":"Sodium","description":"Fast","author":"jelly",
         "downloads":99,"follows":7,"project_type":"mod",
         "display_categories":["optimization","fabric","quilt"],
         "categories":["optimization","fabric","quilt","utility","bad tag!","x/y"],
         "versions":["1.16.5","1.20.1","1.21.1","not a version!"],
         "client_side":"required","server_side":"weird","date_modified":"2026-10-30T10:00:00Z"}
    ]}"#;

    let hit = &parse_search(body).unwrap().hits[0];

    assert_eq!(hit.loaders, ["fabric", "quilt"]);
    assert_eq!(hit.categories, ["optimization", "utility"]);
    assert_eq!(hit.follows, 7);
    assert_eq!(hit.oldest_version.as_deref(), Some("1.16.5"));
    assert_eq!(hit.newest_version.as_deref(), Some("1.21.1"));
    assert_eq!((hit.client_side, hit.server_side), ("required", "unknown"));
    assert_eq!(hit.updated, "2026-10-30T10:00:00Z");
}

#[test]
fn a_hit_with_no_tags_or_versions_is_still_a_hit() {
    let page = parse_search(r#"{"hits":[{"project_id":"AANobbMI","title":"Sodium","project_type":"mod"}]}"#).unwrap();

    let hit = &page.hits[0];
    assert!(hit.loaders.is_empty() && hit.categories.is_empty());
    assert_eq!((hit.oldest_version.clone(), hit.newest_version.clone()), (None, None));
    assert_eq!(hit.client_side, "unknown");
    assert_eq!(hit.follows, 0);
}

#[test]
fn a_search_answer_that_is_not_the_documented_shape_is_refused() {
    assert_eq!(parse_search("nope"), Err(ContentError::Malformed));
    assert_eq!(parse_search("[]"), Err(ContentError::Malformed));
    assert_eq!(
        parse_search(&" ".repeat(MAX_BODY_BYTES + 1)),
        Err(ContentError::TooLarge(MAX_BODY_BYTES))
    );
}

fn version_json(extra_file: &str) -> String {
    format!(
        r#"[{{
        "id":"AbCdEfGh","version_number":"0.6.0","version_type":"release","date_published":"2026-10-01T10:00:00Z",
        "game_versions":["1.21.1","1.21"],"loaders":["fabric","quilt"],
        "files":[{{"url":"https://cdn.modrinth.com/data/AANobbMI/versions/AbCdEfGh/sodium.jar","filename":"sodium.jar",
                  "size":1000,"primary":true,"hashes":{{"sha1":"{SHA1}","sha512":"{SHA512}"}}}}{extra_file}],
        "dependencies":[{{"project_id":"P7dR8mSH","dependency_type":"required"}},
                        {{"project_id":null,"dependency_type":"optional"}},
                        {{"project_id":"Zz9","dependency_type":"weird"}}]
    }}]"#
    )
}

#[test]
fn a_version_keeps_its_checked_files_and_dependencies() {
    let versions = parse_versions(&version_json("")).unwrap();

    assert_eq!(versions.len(), 1);
    let version = &versions[0];
    assert_eq!(version.number, "0.6.0");
    assert_eq!(version.channel, Channel::Release);
    assert_eq!(version.game_versions, ["1.21.1", "1.21"]);
    assert_eq!(version.dependencies.len(), 2);
    assert_eq!(version.dependencies[0].kind, DependencyKind::Required);
    let file = primary_file(version).unwrap();
    assert_eq!(file.filename, "sodium.jar");
    assert_eq!(file.sha1, SHA1);
    assert_eq!(file.sha512.as_deref(), Some(SHA512));
}

#[test]
fn a_file_the_launcher_could_not_check_or_trust_is_not_offered() {
    let bad_files = [
        // not on Modrinth's CDN
        r#"{"url":"https://evil.test/a.jar","filename":"a.jar","size":1,"hashes":{"sha1":"0123456789abcdef0123456789abcdef01234567"}}"#,
        // the CDN name inside another host
        r#"{"url":"https://cdn.modrinth.com.evil.test/a.jar","filename":"a.jar","size":1,"hashes":{"sha1":"0123456789abcdef0123456789abcdef01234567"}}"#,
        // user info
        r#"{"url":"https://cdn.modrinth.com@evil.test/a.jar","filename":"a.jar","size":1,"hashes":{"sha1":"0123456789abcdef0123456789abcdef01234567"}}"#,
        // no hash
        r#"{"url":"https://cdn.modrinth.com/a.jar","filename":"a.jar","size":1,"hashes":{}}"#,
        // a hash that is not one
        r#"{"url":"https://cdn.modrinth.com/a.jar","filename":"a.jar","size":1,"hashes":{"sha1":"XYZ"}}"#,
        // a sha512 that is present but wrong
        r#"{"url":"https://cdn.modrinth.com/a.jar","filename":"a.jar","size":1,"hashes":{"sha1":"0123456789abcdef0123456789abcdef01234567","sha512":"zz"}}"#,
        // path in the name, device name, empty, too big
        r#"{"url":"https://cdn.modrinth.com/a.jar","filename":"../a.jar","size":1,"hashes":{"sha1":"0123456789abcdef0123456789abcdef01234567"}}"#,
        r#"{"url":"https://cdn.modrinth.com/a.jar","filename":"NUL.jar","size":1,"hashes":{"sha1":"0123456789abcdef0123456789abcdef01234567"}}"#,
        r#"{"url":"https://cdn.modrinth.com/a.jar","filename":"a.jar","size":0,"hashes":{"sha1":"0123456789abcdef0123456789abcdef01234567"}}"#,
        r#"{"url":"https://cdn.modrinth.com/a.jar","filename":"a.jar","size":999999999999,"hashes":{"sha1":"0123456789abcdef0123456789abcdef01234567"}}"#,
    ];
    for bad in bad_files {
        let versions = parse_versions(&version_json(&format!(",{bad}"))).unwrap();
        assert_eq!(versions[0].files.len(), 1, "{bad}");
    }

    // a version with nothing usable at all is not a version
    let only_bad = format!(
        r#"[{{"id":"AbCdEfGh","version_type":"release","files":[{}]}}]"#,
        bad_files[0]
    );
    assert!(parse_versions(&only_bad).unwrap().is_empty());
}

#[test]
fn file_names_are_safe_on_every_system() {
    for good in ["sodium-0.6.0.jar", "Faithful 32x.zip", "世界.zip", "a.b.c.jar"] {
        assert!(is_safe_filename(good), "{good}");
    }
    for bad in [
        "", ".", "..", "a/b", "a\\b", "C:x", "a*b", "a?b", "a<b", "a|b", "a\"b", "a\nb", "x.jar.", "x ", " x",
        "CON", "con.txt", "NUL.zip", "COM1.jar", "LPT9", &"a".repeat(129),
    ] {
        assert!(!is_safe_filename(bad), "{bad:?}");
    }
}

fn version(id: &str, channel: Channel, published: &str, games: &[&str], loaders: &[&str]) -> ProjectVersion {
    ProjectVersion {
        id: id.to_owned(),
        number: id.to_owned(),
        channel,
        game_versions: games.iter().map(|g| (*g).to_owned()).collect(),
        loaders: loaders.iter().map(|l| (*l).to_owned()).collect(),
        published: published.to_owned(),
        files: Vec::new(),
        dependencies: Vec::new(),
    }
}

#[test]
fn the_newest_release_that_fits_the_game_and_loader_wins() {
    let versions = [
        version("old", Channel::Release, "2026-01-01", &["1.21.1"], &["fabric"]),
        version("new", Channel::Release, "2026-06-01", &["1.21.1"], &["fabric"]),
        version("beta", Channel::Beta, "2026-09-01", &["1.21.1"], &["fabric"]),
        version("other-game", Channel::Release, "2026-10-01", &["1.20.1"], &["fabric"]),
        version("other-loader", Channel::Release, "2026-10-01", &["1.21.1"], &["forge"]),
    ];

    assert_eq!(pick_version(&versions, "1.21.1", Some("fabric")).unwrap().id, "new");
    // with no loader asked (a resource pack) the other-loader release counts
    assert_eq!(pick_version(&versions, "1.21.1", None).unwrap().id, "other-loader");
}

#[test]
fn a_beta_is_taken_only_when_there_is_no_release() {
    let versions = [
        version("alpha", Channel::Alpha, "2026-10-01", &["1.21.1"], &[]),
        version("beta", Channel::Beta, "2026-05-01", &["1.21.1"], &[]),
    ];
    assert_eq!(pick_version(&versions, "1.21.1", None).unwrap().id, "beta");
    assert!(pick_version(&versions, "1.19", None).is_none());
}

#[test]
fn the_platforms_filter_is_not_taken_on_trust() {
    // the answer lists a version for another game although the request asked for 1.21.1
    let versions = [version("wrong", Channel::Release, "2026-10-01", &["1.16.5"], &["fabric"])];
    assert!(pick_version(&versions, "1.21.1", Some("fabric")).is_none());
}
