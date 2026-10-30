use super::*;

fn sample() -> Tag {
    compound([
        ("byte", Tag::Byte(-3)),
        ("short", Tag::Short(300)),
        ("int", Tag::Int(-70_000)),
        ("long", Tag::Long(1 << 40)),
        ("float", Tag::Float(1.5)),
        ("double", Tag::Double(-2.25)),
        ("bytes", Tag::ByteArray(vec![1, -2, 3])),
        ("name", Tag::String("Stone Bricks 石砖".to_owned())),
        ("ints", Tag::IntArray(vec![1, 2, 3])),
        ("longs", Tag::LongArray(vec![i64::MIN, 0, i64::MAX])),
        ("list", Tag::List(vec![Tag::Int(1), Tag::Int(2)])),
        ("empty", Tag::List(vec![])),
        (
            "nested",
            compound([("deeper", compound([("x", Tag::Int(7))]))]),
        ),
    ])
}

#[test]
fn every_tag_type_survives_a_round_trip_and_keeps_its_order() {
    let original = sample();

    let (name, back) = read(&write("Root", &original)).unwrap();

    assert_eq!(name, "Root");
    assert_eq!(back, original);
}

#[test]
fn gzip_data_is_recognised_and_plain_data_is_accepted() {
    let original = sample();

    assert_eq!(
        read_maybe_gzip(&write_gzip("", &original)).unwrap().1,
        original
    );
    assert_eq!(read_maybe_gzip(&write("", &original)).unwrap().1, original);
}

#[test]
fn a_decompression_bomb_is_refused_instead_of_expanded() {
    // 100 MiB of zeros compresses to a few hundred KiB.
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write as _;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    let zeros = vec![0u8; 1024 * 1024];
    for _ in 0..100 {
        encoder.write_all(&zeros).unwrap();
    }
    let bomb = encoder.finish().unwrap();
    assert!(bomb.len() < 1024 * 1024);

    assert_eq!(decompress(&bomb), Err(NbtError::TooLarge));
}

#[test]
fn damaged_gzip_is_an_error() {
    let mut bytes = write_gzip("", &sample());
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xff;
    bytes.truncate(bytes.len() - 6);
    assert!(read_maybe_gzip(&bytes).is_err());
}

#[test]
fn truncated_data_is_an_error_at_every_length() {
    let bytes = write("", &sample());
    for length in 0..bytes.len() {
        assert!(read(&bytes[..length]).is_err(), "{length}");
    }
}

#[test]
fn data_after_the_structure_is_refused() {
    let mut bytes = write("", &sample());
    bytes.push(0);
    assert_eq!(read(&bytes), Err(NbtError::TrailingData));
}

#[test]
fn the_root_must_be_a_compound() {
    assert_eq!(read(&[8, 0, 0, 0, 0]), Err(NbtError::RootNotCompound));
    assert_eq!(read(&[]), Err(NbtError::Truncated));
}

#[test]
fn an_unknown_tag_type_is_refused() {
    // Root compound, a child of type 99.
    assert_eq!(
        read(&[10, 0, 0, 99, 0, 1, b'a']),
        Err(NbtError::UnknownTag(99))
    );
}

#[test]
fn a_length_larger_than_the_remaining_data_is_refused_before_allocating() {
    // A long array that claims 2^31-1 elements.
    let mut bytes = vec![10, 0, 0, 12, 0, 1, b'a'];
    bytes.extend_from_slice(&i32::MAX.to_be_bytes());
    assert_eq!(read(&bytes), Err(NbtError::BadLength));

    // A negative length.
    let mut bytes = vec![10, 0, 0, 7, 0, 1, b'a'];
    bytes.extend_from_slice(&(-1i32).to_be_bytes());
    assert_eq!(read(&bytes), Err(NbtError::BadLength));

    // A list that claims billions of entries.
    let mut bytes = vec![10, 0, 0, 9, 0, 1, b'a', 3];
    bytes.extend_from_slice(&i32::MAX.to_be_bytes());
    assert_eq!(read(&bytes), Err(NbtError::BadLength));
}

#[test]
fn a_list_of_the_end_type_cannot_claim_elements() {
    let mut bytes = vec![10, 0, 0, 9, 0, 1, b'a', 0];
    bytes.extend_from_slice(&1i32.to_be_bytes());
    bytes.push(0);
    assert_eq!(read(&bytes), Err(NbtError::UnknownTag(0)));
}

#[test]
fn nesting_is_limited() {
    // Compound inside compound, far deeper than allowed.
    let depth = MAX_DEPTH + 10;
    let mut bytes = vec![10, 0, 0];
    for _ in 0..depth {
        bytes.extend_from_slice(&[10, 0, 1, b'a']);
    }
    bytes.extend(std::iter::repeat_n(0u8, depth + 1));
    assert_eq!(read(&bytes), Err(NbtError::TooDeep));

    // Lists nest too.
    let mut bytes = vec![10, 0, 0, 9, 0, 1, b'a'];
    for _ in 0..depth {
        bytes.extend_from_slice(&[9]);
        bytes.extend_from_slice(&1i32.to_be_bytes());
    }
    assert!(read(&bytes).is_err());
}

#[test]
fn a_flood_of_values_is_limited() {
    // One list of MAX_NODES + 1 bytes: each byte becomes a value.
    let count = MAX_NODES + 1;
    let mut bytes = vec![10, 0, 0, 9, 0, 1, b'a', 1];
    bytes.extend_from_slice(&i32::try_from(count).unwrap().to_be_bytes());
    bytes.extend(std::iter::repeat_n(0u8, count));
    bytes.push(0);
    assert_eq!(read(&bytes), Err(NbtError::TooManyNodes));
}

#[test]
fn accessors_find_children_and_convert_numbers() {
    let tag = sample();
    assert_eq!(tag.get("int").and_then(Tag::as_i32), Some(-70_000));
    assert_eq!(tag.get("byte").and_then(Tag::as_i64), Some(-3));
    assert_eq!(tag.get("long").and_then(Tag::as_i32), None);
    assert_eq!(
        tag.get("name").and_then(Tag::as_str),
        Some("Stone Bricks 石砖")
    );
    assert_eq!(
        tag.get("list").and_then(Tag::as_list).map(<[Tag]>::len),
        Some(2)
    );
    assert_eq!(
        tag.get("longs")
            .and_then(Tag::as_long_array)
            .map(<[i64]>::len),
        Some(3)
    );
    assert!(tag.get("missing").is_none());
    assert!(Tag::Int(1).get("x").is_none());
}

#[test]
fn snbt_uses_minecrafts_suffixes_and_quotes_what_needs_it() {
    let tag = compound([
        ("id", Tag::String("minecraft:chest".to_owned())),
        ("Count", Tag::Byte(64)),
        ("weird key", Tag::Short(1)),
        ("big", Tag::Long(5)),
        ("quote", Tag::String("say \"hi\"\n".to_owned())),
        ("ints", Tag::IntArray(vec![1, 2])),
        ("none", Tag::List(vec![])),
    ]);

    assert_eq!(
        to_snbt(&tag, false, 1000),
        "{id: \"minecraft:chest\", Count: 64b, \"weird key\": 1s, big: 5L, \
         quote: \"say \\\"hi\\\"\\n\", ints: [I; 1, 2], none: []}"
    );
}

#[test]
fn pretty_snbt_indents_and_a_huge_value_is_cut() {
    let tag = compound([("a", Tag::List(vec![Tag::Int(1), Tag::Int(2)]))]);
    assert_eq!(
        to_snbt(&tag, true, 1000),
        "{\n  a: [\n    1,\n    2\n  ]\n}"
    );

    let long = Tag::String("x".repeat(10_000));
    let cut = to_snbt(&long, false, 50);
    assert_eq!(cut.chars().count(), 51);
    assert!(cut.ends_with('…'));
}
