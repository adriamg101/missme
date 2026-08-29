use serde::{Deserialize, Serialize};

use crate::models::interpreter::riscv::Base;

/// Frontend strings are "LRU"/"FIFO"/"Random"/"LFU"/"PLRU" — a mix of
/// all-caps acronyms and one capitalized word, not a uniform casing
/// convention, so each variant is renamed explicitly rather than trusting
/// a blanket `rename_all` to get every one right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplacementPolicy {
    #[serde(rename = "LRU")]
    Lru,
    #[serde(rename = "FIFO")]
    Fifo,
    #[serde(rename = "Random")]
    Random,
    #[serde(rename = "LFU")]
    Lfu,
    #[serde(rename = "PLRU")]
    Plru,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TlbReplacementPolicy {
    #[serde(rename = "LRU")]
    Lru,
    #[serde(rename = "FIFO")]
    Fifo,
    #[serde(rename = "Random")]
    Random,
    #[serde(rename = "PLRU")]
    Plru,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WritePolicy {
    WriteThrough,
    WriteBack,
}

/// Frontend strings are "inclusive"/"exclusive"/"NINE" (Non-Inclusive
/// Non-Exclusive) — again mixed casing, explicit renames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InclusionPolicy {
    #[serde(rename = "inclusive")]
    Inclusive,
    #[serde(rename = "exclusive")]
    Exclusive,
    #[serde(rename = "NINE")]
    Nine,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheConfig {
    pub block_size: u32,
    pub associativity: u32,
    pub replacement: ReplacementPolicy,
    pub write_policy: WritePolicy,
    pub write_allocate: bool,
    pub hit_time: u32,
    pub dirty_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEntry {
    pub exp: u32,
    pub config: CacheConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub id: String,
    pub harvard: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unified: Option<CacheEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i: Option<CacheEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub d: Option<CacheEntry>,
    pub inclusion: InclusionPolicy,
    pub bus_width: u32,
}

pub type Hierarchy = Vec<Level>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlbConfig {
    pub page_size: u32,
    pub page_table_levels: u32,
    pub page_walk_start: String,
    pub entries: u32,
    pub associativity: u32,
    pub replacement: TlbReplacementPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RamConfig {
    pub latency: u32,
    pub size: u32,
    pub dirty_percent: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskConfig {
    pub latency: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyConfig {
    pub isa: Base,
    pub vm_enabled: bool,
    pub hierarchy: Hierarchy,
    pub tlb: TlbConfig,
    pub ram: RamConfig,
    pub disk: DiskConfig,
    /// The user's chosen seed for dirty-line pre-seeding, or `None` to draw
    /// a fresh random one each time a session is built — see
    /// `seeded_random` for why nothing here is ever a fixed constant.
    pub seed: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::interpreter::riscv::Base;

    fn sample_cache_config() -> CacheConfig {
        CacheConfig {
            block_size: 64,
            associativity: 2,
            replacement: ReplacementPolicy::Random,
            write_policy: WritePolicy::WriteBack,
            write_allocate: true,
            hit_time: 1,
            dirty_percent: 0,
        }
    }

    #[test]
    fn hierarchy_config_round_trips_and_matches_frontend_json_shape() {
        let config = HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: false,
            hierarchy: vec![Level {
                id: "l1".into(),
                harvard: false,
                unified: Some(CacheEntry { exp: 10, config: sample_cache_config() }),
                i: None,
                d: None,
                inclusion: InclusionPolicy::Nine,
                bus_width: 32,
            }],
            tlb: TlbConfig {
                page_size: 4096,
                page_table_levels: 1,
                page_walk_start: "RAM".into(),
                entries: 64,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0 },
            disk: DiskConfig { latency: 10000 },
            seed: Some(42),
        };

        let json: serde_json::Value = serde_json::to_value(&config).unwrap();

        assert_eq!(json["isa"], "RV32I");
        assert_eq!(json["vmEnabled"], false);
        let level = &json["hierarchy"][0];
        assert_eq!(level["id"], "l1");
        assert_eq!(level["harvard"], false);
        assert_eq!(level["unified"]["exp"], 10);
        assert_eq!(level["unified"]["config"]["blockSize"], 64);
        assert_eq!(level["unified"]["config"]["associativity"], 2);
        assert_eq!(level["unified"]["config"]["replacement"], "Random");
        assert_eq!(level["unified"]["config"]["writePolicy"], "write-back");
        assert_eq!(level["unified"]["config"]["writeAllocate"], true);
        assert_eq!(level["unified"]["config"]["hitTime"], 1);
        assert_eq!(level["unified"]["config"]["dirtyPercent"], 0);
        assert!(level["i"].is_null());
        assert!(level["d"].is_null());
        assert_eq!(level["inclusion"], "NINE");
        assert_eq!(level["busWidth"], 32);
        assert_eq!(json["tlb"]["pageSize"], 4096);
        assert_eq!(json["tlb"]["pageTableLevels"], 1);
        assert_eq!(json["tlb"]["pageWalkStart"], "RAM");
        assert_eq!(json["tlb"]["entries"], 64);
        assert_eq!(json["tlb"]["associativity"], 4);
        assert_eq!(json["tlb"]["replacement"], "LRU");
        assert_eq!(json["ram"]["latency"], 100);
        assert_eq!(json["ram"]["size"], 1024);
        assert_eq!(json["ram"]["dirtyPercent"], 0);
        assert_eq!(json["disk"]["latency"], 10000);
        assert_eq!(json["seed"], 42);

        let back: HierarchyConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn replacement_policy_variants_match_frontend_strings() {
        assert_eq!(serde_json::to_value(ReplacementPolicy::Lru).unwrap(), "LRU");
        assert_eq!(serde_json::to_value(ReplacementPolicy::Fifo).unwrap(), "FIFO");
        assert_eq!(serde_json::to_value(ReplacementPolicy::Random).unwrap(), "Random");
        assert_eq!(serde_json::to_value(ReplacementPolicy::Lfu).unwrap(), "LFU");
        assert_eq!(serde_json::to_value(ReplacementPolicy::Plru).unwrap(), "PLRU");
    }

    #[test]
    fn tlb_replacement_policy_variants_match_frontend_strings() {
        assert_eq!(serde_json::to_value(TlbReplacementPolicy::Lru).unwrap(), "LRU");
        assert_eq!(serde_json::to_value(TlbReplacementPolicy::Fifo).unwrap(), "FIFO");
        assert_eq!(serde_json::to_value(TlbReplacementPolicy::Random).unwrap(), "Random");
        assert_eq!(serde_json::to_value(TlbReplacementPolicy::Plru).unwrap(), "PLRU");
    }

    #[test]
    fn inclusion_policy_variants_match_frontend_strings() {
        assert_eq!(serde_json::to_value(InclusionPolicy::Inclusive).unwrap(), "inclusive");
        assert_eq!(serde_json::to_value(InclusionPolicy::Exclusive).unwrap(), "exclusive");
        assert_eq!(serde_json::to_value(InclusionPolicy::Nine).unwrap(), "NINE");
    }

    #[test]
    fn harvard_level_carries_separate_i_and_d_entries() {
        let level = Level {
            id: "l1".into(),
            harvard: true,
            unified: None,
            i: Some(CacheEntry { exp: 8, config: sample_cache_config() }),
            d: Some(CacheEntry { exp: 9, config: sample_cache_config() }),
            inclusion: InclusionPolicy::Inclusive,
            bus_width: 32,
        };
        let json: serde_json::Value = serde_json::to_value(&level).unwrap();
        assert!(json["unified"].is_null());
        assert_eq!(json["i"]["exp"], 8);
        assert_eq!(json["d"]["exp"], 9);
    }

    #[test]
    fn hierarchy_config_has_exactly_expected_top_level_keys() {
        let config = HierarchyConfig {
            isa: Base::Rv32i,
            vm_enabled: true,
            hierarchy: vec![],
            tlb: TlbConfig {
                page_size: 4096,
                page_table_levels: 1,
                page_walk_start: "RAM".into(),
                entries: 64,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        };

        let json: serde_json::Value = serde_json::to_value(&config).unwrap();
        let obj = json.as_object().expect("HierarchyConfig must serialize to a JSON object");

        let expected_keys = vec!["isa", "vmEnabled", "hierarchy", "tlb", "ram", "disk", "seed"];
        let actual_keys: Vec<&String> = obj.keys().collect();

        assert_eq!(
            actual_keys.len(),
            expected_keys.len(),
            "HierarchyConfig has {} keys but expected {}",
            actual_keys.len(),
            expected_keys.len()
        );

        for key in &expected_keys {
            assert!(obj.contains_key(*key), "Missing expected key: {}", key);
        }

        for key in obj.keys() {
            assert!(
                expected_keys.contains(&key.as_str()),
                "Unexpected key: {}",
                key
            );
        }
    }
}
