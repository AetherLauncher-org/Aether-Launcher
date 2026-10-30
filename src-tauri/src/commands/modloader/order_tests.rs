use super::newest_first;

fn sorted(input: &[&str]) -> Vec<String> {
    let mut versions: Vec<String> = input.iter().map(|version| (*version).to_owned()).collect();
    newest_first(&mut versions);
    versions
}

#[test]
fn maven_order_oldest_first_becomes_newest_first() {
    // Forge's maven-metadata lists oldest first; the picker labels the first entry "latest".
    assert_eq!(
        sorted(&["66.1.0", "66.1.1", "66.1.2"]),
        ["66.1.2", "66.1.1", "66.1.0"]
    );
}

#[test]
fn numbers_compare_as_numbers_not_as_text() {
    assert_eq!(
        sorted(&["47.1.9", "47.1.10", "47.1.100", "47.2.0", "9.9.9"]),
        ["47.2.0", "47.1.100", "47.1.10", "47.1.9", "9.9.9"]
    );
    // Old Forge versions have four parts.
    assert_eq!(
        sorted(&["10.13.4.1448", "10.13.4.1614", "10.13.2.1291"]),
        ["10.13.4.1614", "10.13.4.1448", "10.13.2.1291"]
    );
}

#[test]
fn a_release_is_newer_than_its_own_pre_release() {
    assert_eq!(
        sorted(&["21.0.0-beta", "21.0.0", "20.9.9"]),
        ["21.0.0", "21.0.0-beta", "20.9.9"]
    );
    // A pre-release of a later version still beats an earlier release.
    assert_eq!(
        sorted(&["21.0.0", "21.0.1-beta"]),
        ["21.0.1-beta", "21.0.0"]
    );
}

#[test]
fn missing_parts_count_as_zero() {
    assert_eq!(
        sorted(&["1.2", "1.2.1", "1.2.0"])
            .first()
            .map(String::as_str),
        Some("1.2.1")
    );
}

#[test]
fn odd_input_sorts_without_panicking() {
    let odd = ["", "x", "1.", ".1", "99999999999999999999999.1", "1.0.0"];
    let result = sorted(&odd);
    assert_eq!(result.len(), odd.len());
    // Same input, same order, whatever the starting order.
    let mut reversed = odd;
    reversed.reverse();
    assert_eq!(sorted(&reversed), result);
}

#[test]
fn an_empty_or_single_list_is_left_alone() {
    assert!(sorted(&[]).is_empty());
    assert_eq!(sorted(&["1.0.0"]), ["1.0.0"]);
}
