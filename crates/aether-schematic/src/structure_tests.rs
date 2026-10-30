use super::*;
use crate::model::BlockState;
use crate::nbt::{Tag, compound};

fn sample() -> Schematic {
    let mut schematic = Schematic::empty("Structure", [2, 2, 2]).unwrap();
    schematic.data_version = Some(3955);
    let stone = schematic
        .intern(BlockState::new("minecraft:stone"))
        .unwrap();
    let barrel = schematic
        .intern(BlockState::parse("minecraft:barrel[facing=up]").unwrap())
        .unwrap();
    schematic.blocks[0] = stone;
    let at = schematic.index(1, 1, 1).unwrap();
    schematic.blocks[at] = barrel;
    schematic.block_entities.insert(
        u32::try_from(at).unwrap(),
        compound([("id", Tag::String("minecraft:barrel".to_owned()))]),
    );
    schematic
}

#[test]
fn a_structure_round_trips_and_leaves_air_out_of_the_block_list() {
    let original = sample();

    let tag = to_tag(&original).unwrap();
    let listed = tag.get("blocks").and_then(Tag::as_list).unwrap().len();
    let read_back = read(&tag).unwrap();

    assert_eq!(listed, 2);
    assert_eq!(read_back.size, [2, 2, 2]);
    assert_eq!(read_back.data_version, Some(3955));
    assert_eq!(read_back.solid_count(), 2);
    for index in 0..original.volume() {
        assert_eq!(
            original.palette[original.blocks[index] as usize].to_text(),
            read_back.palette[read_back.blocks[index] as usize].to_text(),
        );
    }
    assert_eq!(read_back.block_entities, original.block_entities);
}

#[test]
fn only_the_first_of_several_palettes_is_read() {
    let variant = |name: &str| Tag::List(vec![compound([("Name", Tag::String(name.to_owned()))])]);
    let tag = compound([
        (
            "size",
            Tag::List(vec![Tag::Int(1), Tag::Int(1), Tag::Int(1)]),
        ),
        (
            "palettes",
            Tag::List(vec![variant("minecraft:stone"), variant("minecraft:dirt")]),
        ),
        (
            "blocks",
            Tag::List(vec![compound([
                (
                    "pos",
                    Tag::List(vec![Tag::Int(0), Tag::Int(0), Tag::Int(0)]),
                ),
                ("state", Tag::Int(0)),
            ])]),
        ),
    ]);

    let schematic = read(&tag).unwrap();

    assert_eq!(
        schematic.palette[schematic.blocks[0] as usize].name,
        "minecraft:stone"
    );
}

#[test]
fn blocks_outside_the_size_or_the_palette_are_corrupt() {
    let make = |pos: [i32; 3], state: i32| {
        compound([
            (
                "size",
                Tag::List(vec![Tag::Int(1), Tag::Int(1), Tag::Int(1)]),
            ),
            (
                "palette",
                Tag::List(vec![compound([(
                    "Name",
                    Tag::String("minecraft:stone".to_owned()),
                )])]),
            ),
            (
                "blocks",
                Tag::List(vec![compound([
                    ("pos", Tag::List(pos.iter().map(|&v| Tag::Int(v)).collect())),
                    ("state", Tag::Int(state)),
                ])]),
            ),
        ])
    };

    assert!(read(&make([0, 0, 0], 0)).is_ok());
    for (pos, state) in [
        ([1, 0, 0], 0),
        ([-1, 0, 0], 0),
        ([0, 0, 0], 1),
        ([0, 0, 0], -1),
    ] {
        assert!(
            matches!(read(&make(pos, state)), Err(SchematicError::Corrupt(_))),
            "{pos:?} {state}"
        );
    }
}

#[test]
fn a_negative_or_huge_size_is_refused() {
    let with_size = |x: i32, y: i32, z: i32| {
        compound([
            (
                "size",
                Tag::List(vec![Tag::Int(x), Tag::Int(y), Tag::Int(z)]),
            ),
            (
                "palette",
                Tag::List(vec![compound([(
                    "Name",
                    Tag::String("minecraft:stone".to_owned()),
                )])]),
            ),
            ("blocks", Tag::List(vec![])),
        ])
    };
    assert!(matches!(
        read(&with_size(-1, 1, 1)),
        Err(SchematicError::Corrupt(_))
    ));
    assert!(matches!(
        read(&with_size(100_000, 1, 1)),
        Err(SchematicError::TooLarge(_))
    ));
    assert!(matches!(
        read(&compound([("x", Tag::Int(1))])),
        Err(SchematicError::Unsupported(_))
    ));
}
