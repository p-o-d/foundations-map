#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SectorId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ObjectId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FactionId(pub u32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_id_equality() {
        assert_eq!(SectorId(1), SectorId(1));
        assert_ne!(SectorId(1), SectorId(2));
    }

    #[test]
    fn ids_are_copy() {
        let id = SectorId(42);
        let _copy = id;
        let _original = id; // both usable — Copy
    }

    #[test]
    fn ids_usable_as_hashmap_keys() {
        let mut map = std::collections::HashMap::new();
        map.insert(SectorId(1), "argon prime");
        assert_eq!(map[&SectorId(1)], "argon prime");
    }
}
