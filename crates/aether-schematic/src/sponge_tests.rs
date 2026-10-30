use super::*;
use crate::model::BlockState;
use crate::nbt::{self, Tag, compound};

fn sample() -> Schematic {
    let mut schematic = Schematic::empty("Sponge sample", [3, 2, 4]).unwrap();
    schematic.author = "Aether".to_owned();
    schematic.data_version = Some(3955);
    let stone = schematic
        .intern(BlockState::new("minecraft:stone"))
        .unwrap();
    let stairs = schematic
        .intern(BlockState::parse("minecraft:oak_stairs[half=bottom,facing=north]").unwrap())
        .unwrap();
    schematic.blocks[0] = stone;
    let at = schematic.index(2, 1, 3).unwrap();
    schematic.blocks[at] = stairs;
    schematic.block_entities.insert(
        u32::try_from(at).unwrap(),
        compound([
            ("id", Tag::String("minecraft:chest".to_owned())),
            ("Lock", Tag::String("key".to_owned())),
        ]),
    );
    schematic
}

#[test]
fn a_version_2_file_round_trips_with_its_states_and_block_entities() {
    let original = sample();

    let read_back = read(&to_tag(&original).unwrap()).unwrap();

    assert_eq!(read_back.size, [3, 2, 4]);
    assert_eq!(read_back.name, "Sponge sample");
    assert_eq!(read_back.data_version, Some(3955));
    for index in 0..original.volume() {
        assert_eq!(
            original.palette[original.blocks[index] as usize].to_text(),
            read_back.palette[read_back.blocks[index] as usize].to_text(),
        );
    }
    assert_eq!(read_back.block_entities, original.block_entities);
}

#[test]
fn a_palette_past_127_entries_uses_multi_byte_indices() {
    let mut schematic = Schematic::empty("Many", [200, 1, 1]).unwrap();
    for x in 0..200u32 {
        let id = schematic
            .intern(BlockState::new(&format!("test:block_{x}")))
            .unwrap();
        schematic.blocks[x as usize] = id;
    }

    let read_back = read(&to_tag(&schematic).unwrap()).unwrap();

    for x in 0..200usize {
        assert_eq!(
            read_back.palette[read_back.blocks[x] as usize].name,
            format!("test:block_{x}")
        );
    }
}

#[test]
fn a_version_3_file_is_read_with_its_nested_blocks_and_entity_data() {
    let tag = compound([(
        "Schematic",
        compound([
            ("Version", Tag::Int(3)),
            ("DataVersion", Tag::Int(3955)),
            ("Width", Tag::Short(2)),
            ("Height", Tag::Short(1)),
            ("Length", Tag::Short(1)),
            (
                "Blocks",
                compound([
                    (
                        "Palette",
                        compound([
                            ("minecraft:air", Tag::Int(0)),
                            ("minecraft:barrel[facing=up,open=false]", Tag::Int(1)),
                        ]),
                    ),
                    ("Data", Tag::ByteArray(vec![0, 1])),
                    (
                        "BlockEntities",
                        Tag::List(vec![compound([
                            ("Id", Tag::String("minecraft:barrel".to_owned())),
                            ("Pos", Tag::IntArray(vec![1, 0, 0])),
                            (
                                "Data",
                                compound([("CustomName", Tag::String("Loot".to_owned()))]),
                            ),
                        ])]),
                    ),
                ]),
            ),
        ]),
    )]);

    let schematic = read(&tag).unwrap();

    assert_eq!(schematic.size, [2, 1, 1]);
    assert_eq!(
        schematic.palette[schematic.blocks[1] as usize].name,
        "minecraft:barrel"
    );
    let entity = schematic.block_entities.get(&1).unwrap();
    assert_eq!(
        entity.get("id").and_then(Tag::as_str),
        Some("minecraft:barrel")
    );
    assert_eq!(entity.get("CustomName").and_then(Tag::as_str), Some("Loot"));
}

#[test]
fn other_versions_and_other_files_are_not_guessed() {
    let tag = compound([("Version", Tag::Int(9))]);
    assert!(matches!(read(&tag), Err(SchematicError::Unsupported(_))));
    assert!(matches!(
        read(&compound([("x", Tag::Int(1))])),
        Err(SchematicError::Unsupported(_))
    ));
}

#[test]
fn block_data_that_does_not_match_the_size_is_corrupt() {
    let make = |data: Vec<i8>| {
        compound([
            ("Version", Tag::Int(2)),
            ("Width", Tag::Short(2)),
            ("Height", Tag::Short(1)),
            ("Length", Tag::Short(1)),
            (
                "Palette",
                compound([
                    ("minecraft:air", Tag::Int(0)),
                    ("minecraft:stone", Tag::Int(1)),
                ]),
            ),
            ("BlockData", Tag::ByteArray(data)),
        ])
    };

    assert!(read(&make(vec![0, 1])).is_ok());
    // Too few, too many, an index past the palette, an unfinished varint, an endless varint.
    for data in [
        vec![0],
        vec![0, 1, 1],
        vec![0, 7],
        vec![0, -128],
        vec![-1, -1, -1, -1, -1, -1, 1],
    ] {
        assert!(
            matches!(read(&make(data.clone())), Err(SchematicError::Corrupt(_))),
            "{data:?}"
        );
    }
}

#[test]
fn a_palette_whose_numbers_are_not_a_range_is_corrupt() {
    let tag = compound([
        ("Version", Tag::Int(2)),
        ("Width", Tag::Short(1)),
        ("Height", Tag::Short(1)),
        ("Length", Tag::Short(1)),
        (
            "Palette",
            compound([
                ("minecraft:air", Tag::Int(0)),
                ("minecraft:stone", Tag::Int(5)),
            ]),
        ),
        ("BlockData", Tag::ByteArray(vec![0])),
    ]);
    assert!(matches!(read(&tag), Err(SchematicError::Corrupt(_))));
}

#[test]
fn a_huge_declared_size_is_refused_before_the_data_is_looked_at() {
    let tag = compound([
        ("Version", Tag::Int(2)),
        ("Width", Tag::Short(-1)), // 65535 as an unsigned short
        ("Height", Tag::Short(100)),
        ("Length", Tag::Short(100)),
        ("Palette", compound([("minecraft:air", Tag::Int(0))])),
        ("BlockData", Tag::ByteArray(vec![])),
    ]);
    assert!(matches!(read(&tag), Err(SchematicError::TooLarge(_))));
}

#[test]
fn the_written_file_has_the_fields_other_tools_look_for() {
    let tag = to_tag(&sample()).unwrap();
    let bytes = nbt::write_gzip("Schematic", &tag);
    let (_, back) = nbt::read_maybe_gzip(&bytes).unwrap();

    assert_eq!(back.get("Version").and_then(Tag::as_i32), Some(2));
    assert!(matches!(back.get("Width"), Some(Tag::Short(3))));
    assert!(back.get("Palette").and_then(Tag::as_compound).is_some());
    assert!(matches!(back.get("BlockData"), Some(Tag::ByteArray(_))));
    assert!(back.get("BlockEntities").and_then(Tag::as_list).is_some());
    // Properties are written sorted, so equal states have one spelling.
    let palette = back.get("Palette").and_then(Tag::as_compound).unwrap();
    assert!(
        palette
            .iter()
            .any(|(key, _)| key == "minecraft:oak_stairs[facing=north,half=bottom]")
    );
}
