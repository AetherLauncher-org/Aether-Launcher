use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_schematic::nbt::{Tag, compound};
use aether_schematic::{BlockState, Format, Schematic, write_as};

use super::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("aether-schematics-cmd-{}-{n}", std::process::id()));
        fs::create_dir_all(path.join("schematics")).unwrap();
        Self(path)
    }

    fn save(&self, name: &str, schematic: &Schematic, format: Format) {
        fs::write(
            self.0.join("schematics").join(name),
            write_as(schematic, format, 0).unwrap(),
        )
        .unwrap();
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn sample() -> Schematic {
    let mut schematic = Schematic::empty("Vault", [4, 2, 2]).unwrap();
    schematic.author = "Aether".to_owned();
    schematic.data_version = Some(3465);
    let grass = schematic
        .intern(BlockState::new("minecraft:grass"))
        .unwrap();
    let chest = schematic
        .intern(BlockState::parse("minecraft:chest[facing=north]").unwrap())
        .unwrap();
    schematic.blocks[0] = grass;
    schematic.blocks[1] = grass;
    let at = schematic.index(3, 1, 1).unwrap();
    schematic.blocks[at] = chest;
    schematic.block_entities.insert(
        u32::try_from(at).unwrap(),
        compound([
            ("id", Tag::String("minecraft:chest".to_owned())),
            ("CustomName", Tag::String("Loot".to_owned())),
            (
                "Items",
                Tag::List(vec![compound([
                    ("Slot", Tag::Byte(4)),
                    ("id", Tag::String("minecraft:diamond_sword".to_owned())),
                    ("Count", Tag::Byte(2)),
                    ("tag", compound([("Damage", Tag::Int(7))])),
                ])]),
            ),
        ]),
    );
    schematic
}

fn decode_base64(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => 0,
    };
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for chunk in bytes.chunks(4) {
        let group = chunk
            .iter()
            .fold(0u32, |acc, &c| (acc << 6) | u32::from(value(c)));
        // `=` already counted as an empty sextet in the fold above.
        let padding = chunk.iter().filter(|&&c| c == b'=').count();
        out.push((group >> 16) as u8);
        if padding < 2 {
            out.push((group >> 8) as u8);
        }
        if padding < 1 {
            out.push(group as u8);
        }
    }
    out
}

#[test]
fn base64_matches_the_rfc_4648_examples() {
    for (plain, encoded) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64(plain.as_bytes()), encoded);
        assert_eq!(decode_base64(encoded), plain.as_bytes());
    }
    let all: Vec<u8> = (0..=255).collect();
    assert_eq!(decode_base64(&base64(&all)), all);
}

#[test]
fn runs_are_value_and_count_pairs_that_expand_back_to_the_blocks() {
    let blocks = [0, 0, 0, 3, 3, 0, 7];

    let runs = encode_runs(&blocks);

    let pairs: Vec<(u32, u32)> = runs
        .chunks(8)
        .map(|c| {
            (
                u32::from_le_bytes(c[..4].try_into().unwrap()),
                u32::from_le_bytes(c[4..].try_into().unwrap()),
            )
        })
        .collect();
    assert_eq!(pairs, [(0, 3), (3, 2), (0, 1), (7, 1)]);
    let expanded: Vec<u32> = pairs
        .iter()
        .flat_map(|&(v, n)| std::iter::repeat_n(v, n as usize))
        .collect();
    assert_eq!(expanded, blocks);
    assert!(encode_runs(&[]).is_empty());
}

#[test]
fn a_view_describes_the_palette_counts_and_block_entities_without_any_path() {
    let view = view_of("Vault.litematic", Format::Litematic, &sample()).unwrap();

    assert_eq!(view.size, [4, 2, 2]);
    assert_eq!(view.solid_blocks, 3);
    assert_eq!(view.data_version, Some(3465));
    assert_eq!(view.format, "litematic");
    let grass = view
        .palette
        .iter()
        .find(|p| p.name == "minecraft:grass")
        .unwrap();
    assert_eq!(grass.count, 2);
    let air = &view.palette[0];
    assert!(air.air && air.count == 13);
    let chest = view
        .palette
        .iter()
        .find(|p| p.name == "minecraft:chest")
        .unwrap();
    assert_eq!(
        chest.properties,
        [Property {
            key: "facing".to_owned(),
            value: "north".to_owned()
        }]
    );

    assert_eq!(view.block_entities.len(), 1);
    let entity = &view.block_entities[0];
    assert_eq!(
        (entity.x, entity.y, entity.z, entity.id.as_str()),
        (3, 1, 1, "minecraft:chest")
    );
    assert!(entity.snbt.contains("CustomName: \"Loot\""));
    assert_eq!(entity.items.len(), 1);
    assert_eq!(entity.items[0].slot, Some(4));
    assert_eq!(entity.items[0].count, 2);
    assert!(
        entity.items[0]
            .data
            .as_deref()
            .unwrap()
            .contains("Damage: 7")
    );

    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["fileName"], "Vault.litematic");
    assert_eq!(json["blockEntityTotal"], 1);
    assert!(json["runs"].is_string() && json["runCount"].as_u64().unwrap() > 0);
    // The expanded runs are exactly the block array.
    let runs = decode_base64(json["runs"].as_str().unwrap());
    assert_eq!(runs, encode_runs(&sample().blocks));
}

#[test]
fn items_in_the_new_format_are_described_too() {
    let mut schematic = sample();
    let at = *schematic.block_entities.keys().next().unwrap();
    schematic.block_entities.insert(
        at,
        compound([
            ("id", Tag::String("minecraft:barrel".to_owned())),
            (
                "Items",
                Tag::List(vec![compound([
                    ("Slot", Tag::Byte(0)),
                    ("id", Tag::String("minecraft:apple".to_owned())),
                    ("count", Tag::Int(64)),
                    (
                        "components",
                        compound([("minecraft:custom_name", Tag::String("Snack".to_owned()))]),
                    ),
                ])]),
            ),
        ]),
    );

    let view = view_of("v.nbt", Format::Structure, &schematic).unwrap();

    assert_eq!(view.block_entities[0].items[0].count, 64);
    assert!(
        view.block_entities[0].items[0]
            .data
            .as_deref()
            .unwrap()
            .contains("Snack")
    );
}

#[test]
fn opening_reads_by_plain_name_and_reports_what_is_wrong() {
    let t = Temp::new();
    t.save("Vault.schem", &sample(), Format::Sponge);
    fs::write(
        t.0.join("schematics/broken.litematic"),
        b"definitely not nbt",
    )
    .unwrap();
    fs::write(
        t.0.join("schematics/other.nbt"),
        aether_schematic::nbt::write_gzip("", &compound([("x", Tag::Int(1))])),
    )
    .unwrap();

    let view = open_schematic(&t.0, "Vault.schem").unwrap();
    assert_eq!(view.format, "schem");
    assert_eq!(view.solid_blocks, 3);

    assert!(open_schematic(&t.0, "broken.litematic").is_err());
    assert!(
        open_schematic(&t.0, "other.nbt")
            .unwrap_err()
            .contains("not a")
    );
    assert!(open_schematic(&t.0, "../Vault.schem").is_err());
    assert!(open_schematic(&t.0, "ghost.nbt").is_err());
}

#[test]
fn converting_saves_a_new_file_beside_the_original_and_keeps_the_original() {
    let t = Temp::new();
    t.save("Vault.litematic", &sample(), Format::Litematic);
    let before = fs::read(t.0.join("schematics/Vault.litematic")).unwrap();

    let result = convert_in(&t.0, "Vault.litematic", "schem", Some(3955)).unwrap();

    assert_eq!(result.file_name, "Vault (converted).schem");
    assert_eq!(result.format, "schem");
    assert_eq!(result.data_version, Some(3955));
    assert_eq!(result.renamed_blocks, 2);
    assert!(!result.warnings.is_empty());
    assert_eq!(
        fs::read(t.0.join("schematics/Vault.litematic")).unwrap(),
        before
    );
    let converted = open_schematic(&t.0, &result.file_name).unwrap();
    assert!(
        converted
            .palette
            .iter()
            .any(|p| p.name == "minecraft:short_grass")
    );

    // A second conversion never overwrites the first.
    let again = convert_in(&t.0, "Vault.litematic", "schem", Some(3955)).unwrap();
    assert_eq!(again.file_name, "Vault (converted) 2.schem");
}

#[test]
fn a_bad_format_version_or_name_converts_nothing() {
    let t = Temp::new();
    t.save("Vault.litematic", &sample(), Format::Litematic);
    let count = || fs::read_dir(t.0.join("schematics")).unwrap().count();

    for (name, format, version) in [
        ("Vault.litematic", "exe", None),
        ("Vault.litematic", "../x", None),
        ("Vault.litematic", "nbt", Some(0)),
        ("Vault.litematic", "nbt", Some(-5)),
        ("Vault.litematic", "nbt", Some(2_000_000_000)),
        ("../Vault.litematic", "nbt", None),
        ("ghost.litematic", "nbt", None),
    ] {
        assert!(
            convert_in(&t.0, name, format, version).is_err(),
            "{name} {format} {version:?}"
        );
    }
    assert_eq!(count(), 1);
}

#[test]
fn the_dialog_gets_the_formats_and_versions_from_the_backend() {
    let targets = targets();

    assert_eq!(
        targets.formats.iter().map(|f| f.label).collect::<Vec<_>>(),
        ["litematic", "schem", "nbt"]
    );
    assert!(
        targets
            .versions
            .iter()
            .any(|v| v.name == "1.21.1" && v.data_version == 3955)
    );
    let json = serde_json::to_value(&targets).unwrap();
    assert_eq!(json["versions"][0]["dataVersion"], 2586);
}
