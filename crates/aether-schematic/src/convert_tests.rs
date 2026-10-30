use super::*;
use crate::model::BlockState;
use crate::nbt::{self, Tag, compound};

fn chest_with(items: Vec<Tag>) -> Tag {
    compound([
        ("id", Tag::String("minecraft:chest".to_owned())),
        ("Items", Tag::List(items)),
    ])
}

fn sample(data_version: Option<i32>) -> Schematic {
    let mut schematic = Schematic::empty("Convert me", [3, 2, 2]).unwrap();
    schematic.author = "Aether".to_owned();
    schematic.data_version = data_version;
    let grass = schematic
        .intern(BlockState::new("minecraft:grass"))
        .unwrap();
    let stairs = schematic
        .intern(BlockState::parse("minecraft:oak_stairs[facing=east,half=top]").unwrap())
        .unwrap();
    let chest = schematic
        .intern(BlockState::parse("minecraft:chest[facing=north]").unwrap())
        .unwrap();
    schematic.blocks[0] = grass;
    schematic.blocks[1] = grass;
    schematic.blocks[2] = stairs;
    let at = schematic.index(2, 1, 1).unwrap();
    schematic.blocks[at] = chest;
    schematic
        .block_entities
        .insert(u32::try_from(at).unwrap(), chest_with(vec![]));
    schematic
}

fn bytes_as(schematic: &Schematic, format: Format) -> Vec<u8> {
    write_as(schematic, format, 0).unwrap()
}

fn same_blocks(a: &Schematic, b: &Schematic) {
    assert_eq!(a.size, b.size);
    for index in 0..a.volume() {
        assert_eq!(
            a.palette[a.blocks[index] as usize].to_text(),
            b.palette[b.blocks[index] as usize].to_text(),
            "block {index}"
        );
    }
}

#[test]
fn every_format_converts_to_every_other_without_changing_a_block() {
    let original = sample(Some(3955));
    for from in Format::ALL {
        for to in Format::ALL {
            let converted = convert(&bytes_as(&original, from), to, None, 0).unwrap();
            assert_eq!(converted.format, to);
            let (format, read_back) = read_any(&converted.bytes).unwrap();
            assert_eq!(format, to, "{from:?} -> {to:?}");
            same_blocks(&original, &read_back);
            assert_eq!(read_back.block_entities.len(), 1, "{from:?} -> {to:?}");
            assert_eq!(read_back.data_version, Some(3955));
            assert_eq!(converted.renamed_blocks, 0);
        }
    }
}

#[test]
fn the_format_is_recognised_from_the_content_not_the_name() {
    let original = sample(Some(3955));
    for format in Format::ALL {
        assert_eq!(read_any(&bytes_as(&original, format)).unwrap().0, format);
    }
    assert_eq!(
        Format::from_file_name("House.LITEMATIC"),
        Some(Format::Litematic)
    );
    assert_eq!(Format::from_file_name("a.schem"), Some(Format::Sponge));
    assert_eq!(Format::from_file_name("a.nbt"), Some(Format::Structure));
    assert_eq!(Format::from_file_name("a.schematic"), None);
    assert_eq!(Format::from_file_name("noextension"), None);
    assert_eq!(Format::parse("schem"), Some(Format::Sponge));
    assert_eq!(Format::parse("../x"), None);
}

#[test]
fn files_that_are_not_schematics_are_reported() {
    let other = nbt::write_gzip("", &compound([("DataVersion", Tag::Int(1))]));
    assert!(matches!(
        read_any(&other),
        Err(SchematicError::Unsupported(_))
    ));

    let legacy = nbt::write_gzip(
        "Schematic",
        &compound([
            ("Width", Tag::Short(1)),
            ("Height", Tag::Short(1)),
            ("Length", Tag::Short(1)),
            ("Blocks", Tag::ByteArray(vec![1])),
        ]),
    );
    let error = read_any(&legacy).unwrap_err();
    assert!(error.to_string().contains("legacy"), "{error}");

    assert!(read_any(b"not nbt at all").is_err());
    assert!(read_any(&[]).is_err());
}

#[test]
fn moving_to_a_newer_version_renames_the_blocks_that_were_renamed() {
    let original = sample(Some(3465)); // 1.20.1

    let converted = convert(
        &bytes_as(&original, Format::Litematic),
        Format::Litematic,
        Some(3955),
        0,
    )
    .unwrap();

    let (_, schematic) = read_any(&converted.bytes).unwrap();
    assert_eq!(schematic.data_version, Some(3955));
    assert_eq!(converted.renamed_blocks, 2);
    let names: Vec<&str> = schematic.palette.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"minecraft:short_grass") && !names.contains(&"minecraft:grass"));
    // Other states are untouched, properties included.
    assert!(
        schematic
            .palette
            .iter()
            .any(|s| s.to_text() == "minecraft:oak_stairs[facing=east,half=top]")
    );
}

#[test]
fn moving_to_an_older_version_undoes_the_rename_and_a_version_in_between_does_nothing() {
    let mut newer = sample(Some(3955));
    newer.palette[1] = BlockState::new("minecraft:short_grass");

    let down = convert(
        &bytes_as(&newer, Format::Sponge),
        Format::Sponge,
        Some(3465),
        0,
    )
    .unwrap();
    let (_, down_schematic) = read_any(&down.bytes).unwrap();
    assert!(
        down_schematic
            .palette
            .iter()
            .any(|s| s.name == "minecraft:grass")
    );
    assert_eq!(down.renamed_blocks, 2);

    // 1.20.1 -> 1.20.2: no rename happened between them.
    let still = sample(Some(3465));
    let across = convert(
        &bytes_as(&still, Format::Sponge),
        Format::Sponge,
        Some(3578),
        0,
    )
    .unwrap();
    assert_eq!(across.renamed_blocks, 0);
}

#[test]
fn container_items_follow_the_1_20_5_format_and_what_cannot_move_is_reported() {
    let mut old = sample(Some(3465));
    let at = *old.block_entities.keys().next().unwrap();
    old.block_entities.insert(
        at,
        chest_with(vec![compound([
            ("Slot", Tag::Byte(2)),
            ("id", Tag::String("minecraft:diamond_sword".to_owned())),
            ("Count", Tag::Byte(1)),
            ("tag", compound([("Damage", Tag::Int(3))])),
        ])]),
    );

    let up = convert(
        &bytes_as(&old, Format::Litematic),
        Format::Litematic,
        Some(3955),
        0,
    )
    .unwrap();

    let (_, upgraded) = read_any(&up.bytes).unwrap();
    let item = &upgraded.block_entities[&at]
        .get("Items")
        .and_then(Tag::as_list)
        .unwrap()[0];
    assert_eq!(item.get("count"), Some(&Tag::Int(1)));
    assert!(item.get("Count").is_none() && item.get("tag").is_none());
    assert_eq!(
        item.get("id").and_then(Tag::as_str),
        Some("minecraft:diamond_sword")
    );
    assert!(
        up.warnings.iter().any(|w| w.contains("Item data")),
        "{:?}",
        up.warnings
    );

    let down = convert(&up.bytes, Format::Litematic, Some(3465), 0).unwrap();
    let (_, back) = read_any(&down.bytes).unwrap();
    let item = &back.block_entities[&at]
        .get("Items")
        .and_then(Tag::as_list)
        .unwrap()[0];
    assert_eq!(item.get("Count"), Some(&Tag::Byte(1)));
    assert!(item.get("count").is_none());
}

#[test]
fn a_count_that_does_not_fit_a_byte_is_clamped_on_the_way_down() {
    let mut new = sample(Some(3955));
    let at = *new.block_entities.keys().next().unwrap();
    new.block_entities.insert(
        at,
        chest_with(vec![compound([
            ("id", Tag::String("minecraft:stone".to_owned())),
            ("count", Tag::Int(5000)),
        ])]),
    );

    let down = convert(
        &bytes_as(&new, Format::Structure),
        Format::Structure,
        Some(3465),
        0,
    )
    .unwrap();

    let (_, back) = read_any(&down.bytes).unwrap();
    let item = &back.block_entities[&at]
        .get("Items")
        .and_then(Tag::as_list)
        .unwrap()[0];
    assert_eq!(item.get("Count"), Some(&Tag::Byte(127)));
}

#[test]
fn an_unknown_source_version_is_said_out_loud() {
    let unknown = sample(None);

    let converted = convert(
        &bytes_as(&unknown, Format::Litematic),
        Format::Litematic,
        Some(3955),
        0,
    )
    .unwrap();

    assert!(
        converted
            .warnings
            .iter()
            .any(|w| w.contains("does not say")),
        "{:?}",
        converted.warnings
    );
    assert_eq!(converted.data_version, Some(3955));
    assert_eq!(converted.renamed_blocks, 0);
}

#[test]
fn without_a_target_version_the_data_version_is_kept_and_no_version_warning_is_given() {
    let converted = convert(
        &bytes_as(&sample(Some(3465)), Format::Litematic),
        Format::Sponge,
        None,
        0,
    )
    .unwrap();

    assert_eq!(converted.data_version, Some(3465));
    assert!(converted.warnings.iter().all(|w| !w.contains("block list")));
}

#[test]
fn signs_crossing_the_1_20_format_change_are_flagged() {
    let mut old = sample(Some(3337));
    let at = *old.block_entities.keys().next().unwrap();
    old.block_entities.insert(
        at,
        compound([("id", Tag::String("minecraft:sign".to_owned()))]),
    );

    let converted = convert(
        &bytes_as(&old, Format::Litematic),
        Format::Litematic,
        Some(3955),
        0,
    )
    .unwrap();

    assert!(
        converted.warnings.iter().any(|w| w.contains("Signs")),
        "{:?}",
        converted.warnings
    );
}

#[test]
fn the_known_versions_are_ascending_and_named() {
    let mut last = 0;
    for &(version, name) in KNOWN_DATA_VERSIONS {
        assert!(version > last && !name.is_empty());
        last = version;
    }
}
