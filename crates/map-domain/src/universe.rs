use crate::ids::{ClusterId, FactionId, SectorId};
use crate::objects::StaticObject;
use glam::Vec2;

#[derive(Debug, Clone)]
pub struct FactionMeta {
    pub display_name: String,
    /// Abbreviated faction name (e.g. "ARG" for Argon), from `factions.xml`
    /// `shortname=`. Prefixes generated object names in-game ("ARG Solar Power
    /// Plant I"). Empty when the faction has no shortname.
    pub short_name: String,
    pub color: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub enum GateType {
    Standard,
    Superhighway,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub from: SectorId,
    pub to: SectorId,
    pub gate_type: GateType,
}

#[derive(Debug, Clone)]
pub struct Sector {
    pub id: SectorId,
    pub name: String,
    pub faction: Option<FactionId>,
    /// Projected from X4 galaxy 3D coords: galaxy x/z → map x/y, y discarded.
    /// This is the *cluster center* — per-sector layout offset is applied by the
    /// renderer in pixel space so it scales with `hex_r` rather than zoom.
    pub map_position: Vec2,
    pub static_objects: Vec<StaticObject>,
    pub cluster_id: Option<ClusterId>,
    pub index_in_cluster: u32,
    pub cluster_sector_count: u32,
}

#[derive(Debug, Clone)]
pub struct Cluster {
    pub id: ClusterId,
    pub name: String,
    /// Cluster center in map space (same units as Sector.map_position).
    pub map_position: Vec2,
    /// Encompassing radius in map units — covers all sectors in this cluster plus a margin.
    pub radius: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Universe {
    pub sectors: Vec<Sector>,
    pub clusters: Vec<Cluster>,
    pub connections: Vec<Connection>,
    /// Lowercase sector macro → SectorId, for resolving save-file references.
    pub sector_macros: std::collections::HashMap<String, SectorId>,
    /// Lowercase X4 faction id (e.g. "argon") → FactionId. Built at static load.
    pub faction_strings: std::collections::HashMap<String, FactionId>,
    /// FactionId → resolved display name + game palette colour.
    pub faction_table: std::collections::HashMap<FactionId, FactionMeta>,
    /// Lowercase ware id (e.g. "energycells") → display name (e.g. "Energy Cells").
    /// Built once at galaxy load from `libraries/wares.xml`. Empty if parse failed.
    pub ware_names: std::collections::HashMap<String, String>,
    /// Lowercase ware id → station/factory display name from `wares.xml`
    /// `factoryname=` (e.g. "energycells" → "Solar Power Plant"). This is the
    /// name the game shows for an NPC factory producing that ware, distinct from
    /// the ware name itself. Empty if absent.
    pub ware_factory_names: std::collections::HashMap<String, String>,
    /// Lowercase production-module macro → the ware it produces (e.g.
    /// "prod_gen_energycells_macro" → "energycells"), from each module macro's
    /// `<production wares="..."/>`. Lets a station resolve its factory name via
    /// [`ware_factory_names`].
    pub prod_module_wares: std::collections::HashMap<String, String>,
    /// Lowercase job id (e.g. "argon_construction_vessel_xl_focused") → resolved
    /// job display name (e.g. "Builder Ship"), from `libraries/jobs.xml`
    /// `<job name="{p,t}"/>`. NPC ships are prefixed in-game with their job name.
    pub job_names: std::collections::HashMap<String, String>,
    /// Full translation table: (page_id, text_id) -> display string. Populated at
    /// galaxy load from `t/0001-l<locale>.xml`. Consumed by the entity-label
    /// resolver and the ware-name lookup.
    pub translations: std::collections::HashMap<(u32, u32), String>,
    /// Locale IDs the installed game ships (parsed from `t/0001-l*.xml` filenames).
    /// Drives the top-bar language dropdown.
    pub available_locales: Vec<u32>,
    /// Locale ID this Universe was loaded with (e.g. 44 for English).
    pub current_locale: u32,
    /// Lowercase macro name (e.g. "ship_par_l_trans_container_03_a_macro") →
    /// raw `<identification name=...>` ref from the macro definition file
    /// (e.g. "{20101,30804}"). Resolved at display time via translations.
    /// Used as a fallback when a save entity carries no per-instance name=.
    pub macro_identifications: std::collections::HashMap<String, String>,
    /// Lowercase zone macro → sector-relative position in metres. Object/gate
    /// offsets in zones.xml and the save are zone-relative; adding the zone's
    /// position here yields the true sector-relative position. Used by the save
    /// parser to place live ships/stations consistently with static gates.
    pub zone_positions: std::collections::HashMap<String, (f32, f32, f32)>,
    /// Per-sector mineable resource areas, parsed from mapdefaults
    /// `<resourceareas>`. Drives the "Resources" map filter. Empty/missing for
    /// sectors with no yields.
    pub sector_resources:
        std::collections::HashMap<SectorId, Vec<crate::resources::SectorResource>>,
}

impl Universe {
    pub fn sector(&self, id: SectorId) -> Option<&Sector> {
        self.sectors.iter().find(|s| s.id == id)
    }

    pub fn connections_for(&self, id: SectorId) -> Vec<&Connection> {
        self.connections
            .iter()
            .filter(|c| c.from == id || c.to == id)
            .collect()
    }

    pub fn neighbour_ids(&self, id: SectorId) -> Vec<SectorId> {
        self.connections_for(id)
            .iter()
            .map(|c| if c.from == id { c.to } else { c.from })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_universe() -> Universe {
        let a = SectorId(1);
        let b = SectorId(2);
        Universe {
            sector_macros: HashMap::new(),
            faction_strings: HashMap::new(),
            faction_table: HashMap::new(),
            ware_names: HashMap::new(),
            ware_factory_names: HashMap::new(),
            prod_module_wares: HashMap::new(),
            job_names: HashMap::new(),
            translations: HashMap::new(),
            available_locales: Vec::new(),
            current_locale: 0,
            macro_identifications: HashMap::new(),
            zone_positions: HashMap::new(),
            sector_resources: HashMap::new(),
            sectors: vec![
                Sector {
                    id: a,
                    name: "Argon Prime".into(),
                    faction: Some(FactionId(1)),
                    map_position: Vec2::new(0.0, 0.0),
                    static_objects: vec![],
                    cluster_id: None,
                    index_in_cluster: 0,
                    cluster_sector_count: 1,
                },
                Sector {
                    id: b,
                    name: "Hatikvah's Choice I".into(),
                    faction: Some(FactionId(2)),
                    map_position: Vec2::new(1.0, 0.5),
                    static_objects: vec![],
                    cluster_id: None,
                    index_in_cluster: 0,
                    cluster_sector_count: 1,
                },
            ],
            clusters: vec![],
            connections: vec![Connection {
                from: a,
                to: b,
                gate_type: GateType::Standard,
            }],
        }
    }

    #[test]
    fn sector_lookup_by_id() {
        let u = make_universe();
        assert_eq!(u.sector(SectorId(1)).unwrap().name, "Argon Prime");
        assert!(u.sector(SectorId(99)).is_none());
    }

    #[test]
    fn connections_for_returns_both_sides() {
        let u = make_universe();
        assert_eq!(u.connections_for(SectorId(1)).len(), 1);
        assert_eq!(u.connections_for(SectorId(2)).len(), 1);
        assert_eq!(u.connections_for(SectorId(99)).len(), 0);
    }

    #[test]
    fn neighbour_ids_from_both_directions() {
        let u = make_universe();
        assert_eq!(u.neighbour_ids(SectorId(1)), vec![SectorId(2)]);
        assert_eq!(u.neighbour_ids(SectorId(2)), vec![SectorId(1)]);
    }

    #[test]
    fn faction_table_holds_meta() {
        let mut u = Universe::default();
        u.faction_strings.insert("argon".into(), FactionId(1));
        u.faction_table.insert(
            FactionId(1),
            FactionMeta {
                display_name: "Argon Federation".into(),
                short_name: "ARG".into(),
                color: [50, 120, 255, 255],
            },
        );
        assert_eq!(u.faction_strings.get("argon"), Some(&FactionId(1)));
        assert_eq!(
            u.faction_table.get(&FactionId(1)).unwrap().display_name,
            "Argon Federation"
        );
    }

    #[test]
    fn ware_names_can_be_populated() {
        let mut u = Universe::default();
        u.ware_names
            .insert("energycells".into(), "Energy Cells".into());
        assert_eq!(
            u.ware_names.get("energycells").map(String::as_str),
            Some("Energy Cells")
        );
        assert!(u.ware_names.get("missing").is_none());
    }

    #[test]
    fn universe_translations_and_locale_fields() {
        let mut u = Universe::default();
        u.translations
            .insert((20101, 122701), "Cerberus Vanguard".into());
        u.available_locales = vec![44, 49, 33];
        u.current_locale = 44;
        assert_eq!(
            u.translations.get(&(20101, 122701)).map(String::as_str),
            Some("Cerberus Vanguard")
        );
        assert_eq!(u.available_locales, vec![44, 49, 33]);
        assert_eq!(u.current_locale, 44);
    }

    #[test]
    fn macro_identifications_round_trip() {
        let mut u = Universe::default();
        u.macro_identifications.insert(
            "ship_par_l_trans_container_03_a_macro".into(),
            "{20101,30804}".into(),
        );
        assert_eq!(
            u.macro_identifications
                .get("ship_par_l_trans_container_03_a_macro")
                .map(String::as_str),
            Some("{20101,30804}")
        );
        assert!(u.macro_identifications.get("missing").is_none());
    }
}
