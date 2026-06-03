//! Per-sector mineable resource data.
//!
//! As of X4 9.00 resource yields are declared per sector in
//! `libraries/mapdefaults.xml` under `<properties><resourceareas>`, one
//! `<resourcearea amount="N" ref="sphere_<size>_<ware>_<tier>_<speed>"/>` per
//! field. We keep the parsed `(ware, tier, amount)` triples and combine
//! same-ware areas for display.

/// Richness band of a resource area. `Ord` runs verylow → veryhigh so `.max()`
/// picks the richest band present in a sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceTier {
    VeryLow,
    Low,
    Medium,
    High,
    VeryHigh,
}

impl ResourceTier {
    /// Parse the tier token from a `resourcearea` ref (e.g. `"veryhigh"`).
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "verylow" => ResourceTier::VeryLow,
            "low" => ResourceTier::Low,
            "medium" => ResourceTier::Medium,
            "high" => ResourceTier::High,
            "veryhigh" => ResourceTier::VeryHigh,
            _ => return None,
        })
    }

    pub fn label(&self) -> &'static str {
        match self {
            ResourceTier::VeryLow => "Very Low",
            ResourceTier::Low => "Low",
            ResourceTier::Medium => "Medium",
            ResourceTier::High => "High",
            ResourceTier::VeryHigh => "Very High",
        }
    }
}

/// One resource entry for a sector. When stored on `Universe.sector_resources`
/// it is a raw per-area record; after `combine_resources` it is one aggregated
/// row per ware (richest tier, summed amount).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectorResource {
    /// Lowercase X4 ware id (e.g. `"ore"`). Display name resolved via
    /// `Universe.ware_names` at render time.
    pub ware: String,
    pub tier: ResourceTier,
    pub amount: u32,
}

/// Collapse all areas of the same ware into one row: richest tier + summed
/// amount. Result sorted by ware id for stable display order.
pub fn combine_resources(areas: &[SectorResource]) -> Vec<SectorResource> {
    use std::collections::BTreeMap;
    let mut by_ware: BTreeMap<&str, (ResourceTier, u32)> = BTreeMap::new();
    for a in areas {
        let entry = by_ware.entry(a.ware.as_str()).or_insert((a.tier, 0));
        entry.0 = entry.0.max(a.tier);
        entry.1 += a.amount;
    }
    by_ware
        .into_iter()
        .map(|(ware, (tier, amount))| SectorResource {
            ware: ware.to_string(),
            tier,
            amount,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_parse_and_order() {
        assert_eq!(
            ResourceTier::parse("veryhigh"),
            Some(ResourceTier::VeryHigh)
        );
        assert_eq!(ResourceTier::parse("nope"), None);
        assert!(ResourceTier::VeryLow < ResourceTier::High);
        assert_eq!(
            ResourceTier::High.max(ResourceTier::Low),
            ResourceTier::High
        );
    }

    #[test]
    fn combine_picks_max_tier_and_sums_amount() {
        let areas = vec![
            SectorResource {
                ware: "ore".into(),
                tier: ResourceTier::Low,
                amount: 4,
            },
            SectorResource {
                ware: "ore".into(),
                tier: ResourceTier::High,
                amount: 3,
            },
            SectorResource {
                ware: "ice".into(),
                tier: ResourceTier::Medium,
                amount: 2,
            },
        ];
        let out = combine_resources(&areas);
        // Sorted by ware: ice, ore.
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0],
            SectorResource {
                ware: "ice".into(),
                tier: ResourceTier::Medium,
                amount: 2
            }
        );
        assert_eq!(
            out[1],
            SectorResource {
                ware: "ore".into(),
                tier: ResourceTier::High,
                amount: 7
            }
        );
    }

    #[test]
    fn combine_empty_is_empty() {
        assert!(combine_resources(&[]).is_empty());
    }
}
