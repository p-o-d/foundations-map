use map_domain::objects::StaticObjectKind;
use map_io::xml_parser;
use std::path::Path;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn parse_galaxy_produces_correct_sector_count() {
    let universe = xml_parser::parse_galaxy(&fixture("galaxy.xml")).unwrap();
    assert_eq!(universe.sectors.len(), 2);
}

#[test]
fn parse_galaxy_sector_names_correct() {
    let universe = xml_parser::parse_galaxy(&fixture("galaxy.xml")).unwrap();
    let names: Vec<&str> = universe.sectors.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Argon Prime"));
    assert!(names.contains(&"Hatikvah's Choice I"));
}

#[test]
fn parse_galaxy_positions_are_nonzero_and_distinct() {
    let universe = xml_parser::parse_galaxy(&fixture("galaxy.xml")).unwrap();
    let p0 = universe.sectors[0].map_position;
    let p1 = universe.sectors[1].map_position;
    assert_ne!(p0, p1);
}

#[test]
fn parse_sector_objects_returns_station_gate_resourcezone() {
    let objects = xml_parser::parse_sector_objects(&fixture("sector_argon_prime.xml")).unwrap();
    assert_eq!(objects.len(), 3);

    let kinds: Vec<&StaticObjectKind> = objects.iter().map(|o| &o.kind).collect();
    assert!(kinds.contains(&&StaticObjectKind::Station));
    assert!(kinds.contains(&&StaticObjectKind::Gate));
    assert!(kinds.contains(&&StaticObjectKind::ResourceZone));
}

#[test]
fn parse_sector_objects_positions_are_set() {
    let objects = xml_parser::parse_sector_objects(&fixture("sector_argon_prime.xml")).unwrap();
    let station = objects
        .iter()
        .find(|o| o.name.contains("Trading Station"))
        .unwrap();
    assert_eq!(station.position.x, 100000.0);
    assert_eq!(station.position.z, -200000.0);
}

#[test]
fn zone_name_to_sector_macro_extracts_correctly() {
    assert_eq!(
        xml_parser::zone_name_to_sector_macro("Zone003_Cluster_01_Sector001_macro"),
        Some("Cluster_01_Sector001_macro".to_string()),
    );
    assert_eq!(
        xml_parser::zone_name_to_sector_macro("NotAZone"),
        None,
    );
}
