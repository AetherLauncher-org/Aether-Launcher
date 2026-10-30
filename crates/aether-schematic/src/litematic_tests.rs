use super::*;
use crate::model::BlockState;
use crate::nbt::{self, Tag, compound};

fn state(name: &str, properties: &[(&str, &str)]) -> BlockState {
    BlockState {
        name: name.to_owned(),
        properties: properties
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    }
}

fn chest() -> Tag {
    compound([
        ("id", Tag::String("minecraft:chest".to_owned())),
        (
            "Items",
            Tag::List(vec![compound([
                ("Slot", Tag::Byte(3)),
                ("id", Tag::String("minecraft:diamond".to_owned())),
                ("count", Tag::Int(5)),
            ])]),
        ),
    ])
}

fn sample() -> Schematic {
    let mut schematic = Schematic::empty("Sample", [4, 3, 5]).unwrap();
    schematic.author = "Aether".to_owned();
    schematic.description = "Round trip".to_owned();
    schematic.data_version = Some(3955);
    let stone = schematic
        .intern(BlockState::new("minecraft:stone"))
        .unwrap();
    let stairs = schematic
        .intern(state(
            "minecraft:oak_stairs",
            &[("facing", "north"), ("half", "bottom")],
        ))
        .unwrap();
    let chest_state = schematic
        .intern(state("minecraft:chest", &[("facing", "east")]))
        .unwrap();
    let glass = schematic
        .intern(BlockState::new("minecraft:glass"))
        .unwrap();
    for x in 0..4 {
        let at = schematic.index(x, 0, 0).unwrap();
        schematic.blocks[at] = stone;
    }
    let at = schematic.index(1, 1, 2).unwrap();
    schematic.blocks[at] = stairs;
    let at = schematic.index(3, 2, 4).unwrap();
    schematic.blocks[at] = chest_state;
    schematic
        .block_entities
        .insert(u32::try_from(at).unwrap(), chest());
    let at = schematic.index(0, 2, 0).unwrap();
    schematic.blocks[at] = glass;
    schematic
}

/// Packs values the way Litematica does, independently of the writer under test.
fn pack(values: &[u32], palette_len: usize) -> Vec<i64> {
    let bits = bits_for(palette_len) as usize;
    let mut longs = vec![0u64; (values.len() * bits).div_ceil(64)];
    for (i, &value) in values.iter().enumerate() {
        for bit in 0..bits {
            if value >> bit & 1 == 1 {
                let position = i * bits + bit;
                longs[position / 64] |= 1 << (position % 64);
            }
        }
    }
    longs.into_iter().map(u64::cast_signed).collect()
}

fn region(
    position: [i32; 3],
    size: [i32; 3],
    palette: &[&str],
    values: &[u32],
    entities: Vec<Tag>,
) -> Tag {
    compound([
        ("Position", xyz(position[0], position[1], position[2])),
        ("Size", xyz(size[0], size[1], size[2])),
        (
            "BlockStatePalette",
            Tag::List(
                palette
                    .iter()
                    .map(|name| compound([("Name", Tag::String((*name).to_owned()))]))
                    .collect(),
            ),
        ),
        ("BlockStates", Tag::LongArray(pack(values, palette.len()))),
        ("TileEntities", Tag::List(entities)),
    ])
}

fn file(version: i32, regions: Vec<(&str, Tag)>) -> Tag {
    Tag::Compound(vec![
        ("Version".to_owned(), Tag::Int(version)),
        ("MinecraftDataVersion".to_owned(), Tag::Int(3465)),
        (
            "Metadata".to_owned(),
            compound([("Name", Tag::String("Hand made".to_owned()))]),
        ),
        (
            "Regions".to_owned(),
            Tag::Compound(
                regions
                    .into_iter()
                    .map(|(n, t)| (n.to_owned(), t))
                    .collect(),
            ),
        ),
    ])
}

#[test]
fn a_schematic_survives_writing_and_reading_through_gzip() {
    let original = sample();

    let bytes = nbt::write_gzip("", &to_tag(&original, 1_700_000_000_000).unwrap());
    let read_back = read(&nbt::read_maybe_gzip(&bytes).unwrap().1).unwrap();

    assert_eq!(read_back.name, "Sample");
    assert_eq!(read_back.author, "Aether");
    assert_eq!(read_back.data_version, Some(3955));
    assert_eq!(read_back.size, original.size);
    assert_eq!(read_back.solid_count(), original.solid_count());
    // Same block in the same place, whatever order the palettes ended up in.
    for index in 0..original.volume() {
        let before = &original.palette[original.blocks[index] as usize];
        let after = &read_back.palette[read_back.blocks[index] as usize];
        assert_eq!(before.to_text(), after.to_text(), "block {index}");
    }
    assert_eq!(read_back.block_entities, original.block_entities);
}

#[test]
fn entries_that_span_two_longs_are_packed_and_unpacked_exactly() {
    // 5 states need 3 bits, which does not divide 64, so entries straddle a long.
    let mut schematic = Schematic::empty("Packed", [7, 3, 3]).unwrap();
    let names = ["a", "b", "c", "d"];
    let ids: Vec<u32> = names
        .iter()
        .map(|n| {
            schematic
                .intern(BlockState::new(&format!("minecraft:{n}")))
                .unwrap()
        })
        .collect();
    for i in 0..schematic.volume() {
        schematic.blocks[i] = if i % 5 == 0 { 0 } else { ids[i % 4] };
    }
    assert_eq!(bits_for(schematic.palette.len()), 3);

    let tag = to_tag(&schematic, 0).unwrap();
    let read_back = read(&tag).unwrap();

    assert_eq!(read_back.blocks.len(), 63);
    for i in 0..63 {
        assert_eq!(
            schematic.palette[schematic.blocks[i] as usize].name,
            read_back.palette[read_back.blocks[i] as usize].name,
            "{i}"
        );
    }
}

#[test]
fn a_file_from_another_writer_is_read() {
    let bytes = include_bytes!("../fixtures/small_house.litematic");

    let schematic = read(&nbt::read_maybe_gzip(bytes).unwrap().1).unwrap();

    assert_eq!(schematic.name, "Small House");
    assert_eq!(schematic.size, [7, 6, 7]);
    assert_eq!(schematic.data_version, Some(3955));
    assert_eq!(schematic.solid_count(), 193);
    let names: Vec<&str> = schematic.palette.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"minecraft:cobblestone") && names.contains(&"minecraft:glass"));
    // The floor is planks; the door opening in the front wall is air.
    let floor = schematic.index(3, 0, 3).unwrap();
    assert_eq!(
        schematic.palette[schematic.blocks[floor] as usize].name,
        "minecraft:oak_planks"
    );
    let door = schematic.index(3, 1, 0).unwrap();
    assert!(schematic.palette[schematic.blocks[door] as usize].is_air());
}

#[test]
fn a_backwards_region_starts_at_position_plus_size_plus_one() {
    // Position (10, 5, 10), size (-2, 1, -2): it covers x 9..=10, z 9..=10.
    let values = [1, 2, 1, 2];
    let tag = file(
        6,
        vec![(
            "r",
            region(
                [10, 5, 10],
                [-2, 1, -2],
                &["minecraft:air", "minecraft:stone", "minecraft:dirt"],
                &values,
                vec![],
            ),
        )],
    );

    let schematic = read(&tag).unwrap();

    assert_eq!(schematic.size, [2, 1, 2]);
    let at = |x, z| {
        schematic.palette[schematic.blocks[schematic.index(x, 0, z).unwrap()] as usize]
            .name
            .clone()
    };
    assert_eq!(at(0, 0), "minecraft:stone");
    assert_eq!(at(1, 0), "minecraft:dirt");
    assert_eq!(at(0, 1), "minecraft:stone");
}

#[test]
fn two_regions_are_merged_into_one_box_and_air_does_not_erase() {
    let palette = ["minecraft:air", "minecraft:stone", "minecraft:gold_block"];
    let first = region([0, 0, 0], [2, 1, 1], &palette, &[1, 1], vec![]);
    // Overlaps x = 1 with air, so the stone there stays.
    let second = region([1, 0, 0], [3, 1, 1], &palette, &[0, 2, 2], vec![]);

    let schematic = read(&file(6, vec![("a", first), ("b", second)])).unwrap();

    assert_eq!(schematic.size, [4, 1, 1]);
    let names: Vec<String> = (0..4)
        .map(|x| {
            schematic.palette[schematic.blocks[schematic.index(x, 0, 0).unwrap()] as usize]
                .name
                .clone()
        })
        .collect();
    assert_eq!(
        names,
        [
            "minecraft:stone",
            "minecraft:stone",
            "minecraft:gold_block",
            "minecraft:gold_block"
        ]
    );
}

#[test]
fn tile_entities_are_placed_in_the_merged_box_and_lose_their_coordinates() {
    let palette = ["minecraft:air", "minecraft:chest"];
    let in_box = Tag::Compound(vec![
        ("x".to_owned(), Tag::Int(1)),
        ("y".to_owned(), Tag::Int(0)),
        ("z".to_owned(), Tag::Int(0)),
        ("id".to_owned(), Tag::String("minecraft:chest".to_owned())),
    ]);
    let outside = Tag::Compound(vec![
        ("x".to_owned(), Tag::Int(9)),
        ("y".to_owned(), Tag::Int(0)),
        ("z".to_owned(), Tag::Int(0)),
        ("id".to_owned(), Tag::String("minecraft:chest".to_owned())),
    ]);
    // The region is at x = 5, so the entity at local x = 1 lands at box x = 1.
    let tag = file(
        6,
        vec![(
            "r",
            region(
                [5, 0, 0],
                [2, 1, 1],
                &palette,
                &[0, 1],
                vec![in_box, outside],
            ),
        )],
    );

    let schematic = read(&tag).unwrap();

    assert_eq!(schematic.block_entities.len(), 1);
    let (index, data) = schematic.block_entities.iter().next().unwrap();
    assert_eq!(*index, 1);
    assert!(data.get("x").is_none());
    assert_eq!(
        data.get("id").and_then(Tag::as_str),
        Some("minecraft:chest")
    );
}

#[test]
fn other_format_versions_are_reported_not_guessed() {
    for version in [0, 1, 4, 8, 99] {
        let tag = file(version, vec![]);
        let error = read(&tag).unwrap_err();
        assert!(
            matches!(error, SchematicError::Unsupported(ref text) if text.contains(&version.to_string())),
            "{error}"
        );
    }
    assert!(matches!(
        read(&compound([("something", Tag::Int(1))])),
        Err(SchematicError::Unsupported(_))
    ));
}

#[test]
fn a_file_without_regions_or_with_broken_regions_is_corrupt() {
    assert!(matches!(
        read(&file(6, vec![])),
        Err(SchematicError::Corrupt(_))
    ));

    // Block data shorter than the size needs.
    let mut short = region(
        [0, 0, 0],
        [4, 4, 4],
        &["minecraft:air", "minecraft:stone"],
        &[0; 64],
        vec![],
    );
    if let Tag::Compound(entries) = &mut short {
        for (key, value) in entries.iter_mut() {
            if key == "BlockStates" {
                *value = Tag::LongArray(vec![0]);
            }
        }
    }
    assert!(matches!(
        read(&file(6, vec![("r", short)])),
        Err(SchematicError::Corrupt(_))
    ));

    // An index past the palette (2 entries, value 3 fits in 2 bits).
    let bad = region(
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[0, 3],
        vec![],
    );
    assert!(matches!(
        read(&file(6, vec![("r", bad)])),
        Err(SchematicError::Corrupt(_))
    ));

    // A zero-sized region.
    let empty = region([0, 0, 0], [0, 1, 1], &["minecraft:air"], &[], vec![]);
    assert!(matches!(
        read(&file(6, vec![("r", empty)])),
        Err(SchematicError::Corrupt(_))
    ));
}

#[test]
fn oversized_regions_and_far_apart_regions_are_refused_before_allocating() {
    let huge = region([0, 0, 0], [5000, 1, 1], &["minecraft:air"], &[], vec![]);
    assert!(matches!(
        read(&file(6, vec![("r", huge)])),
        Err(SchematicError::TooLarge(_))
    ));

    let a = region(
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1],
        vec![],
    );
    let b = region(
        [2_000_000_000, 0, 0],
        [1, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1],
        vec![],
    );
    assert!(matches!(
        read(&file(6, vec![("a", a), ("b", b)])),
        Err(SchematicError::TooLarge(_))
    ));
}

#[test]
fn writing_refuses_a_schematic_that_does_not_hold_together() {
    let mut broken = sample();
    broken.blocks[0] = 99;
    assert!(to_tag(&broken, 0).is_err());

    let mut stray = sample();
    stray.block_entities.insert(99_999, chest());
    assert!(to_tag(&stray, 0).is_err());
}

#[test]
fn unused_palette_entries_are_dropped_and_air_comes_first() {
    let mut schematic = Schematic::empty("Compact", [2, 1, 1]).unwrap();
    let _unused = schematic.intern(BlockState::new("minecraft:dirt")).unwrap();
    let stone = schematic
        .intern(BlockState::new("minecraft:stone"))
        .unwrap();
    schematic.blocks[1] = stone;

    let tag = to_tag(&schematic, 0).unwrap();
    let palette = tag
        .get("Regions")
        .and_then(|r| r.as_compound())
        .and_then(|r| r[0].1.get("BlockStatePalette"))
        .and_then(Tag::as_list)
        .unwrap();

    let names: Vec<&str> = palette
        .iter()
        .filter_map(|p| p.get("Name").and_then(Tag::as_str))
        .collect();
    assert_eq!(names, ["minecraft:air", "minecraft:stone"]);
}
