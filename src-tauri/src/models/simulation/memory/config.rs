use serde::{Deserialize, Serialize};

use crate::models::interpreter::riscv::Base;

/// The interface sends these as the exact strings LRU, FIFO, Random, LFU, PLRU.
/// Each one is spelled out by hand since they mix different casings.
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
    #[serde(rename = "LFU")]
    Lfu,
    #[serde(rename = "PLRU")]
    Plru,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WritePolicy {
    WriteThrough,
    WriteBack,
}

/// The interface sends these as inclusive, exclusive, or NINE, each spelled out by hand due to mixed casing.
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
    pub page_walk_start: String,
    pub entries: u32,
    pub associativity: u32,
    pub replacement: TlbReplacementPolicy,
    pub latency: u32,
}

/// Which free frame a page fault picks, either the next one in order or a seeded random pick.
/// Once every frame is full, eviction always picks the least recently used one instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrameAllocationPolicy {
    Sequential,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RamConfig {
    pub latency: u32,
    pub size: u32,
    pub dirty_percent: u8,
    pub page_table_levels: u8,
    pub frame_allocation: FrameAllocationPolicy,
    /// How many frames the physical frame pool has under VM, a power of two the user picks directly.
    /// Only meaningful and only shown when VM is on.
    pub frames: u32,
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
    pub page_size: u32,
    pub hierarchy: Hierarchy,
    pub tlb: TlbConfig,
    pub ram: RamConfig,
    pub disk: DiskConfig,
    /// The seed the user chose for randomly marking some cache lines dirty ahead of time, or none to pick a fresh one each session.
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
            page_size: 4096,
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
                page_walk_start: "RAM".into(),
                entries: 64,
                associativity: 4,
                replacement: TlbReplacementPolicy::Lru,
                latency: 1,
            },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0, page_table_levels: 1, frame_allocation: FrameAllocationPolicy::Sequential, frames: 4 },
            disk: DiskConfig { latency: 10000 },
            seed: Some(42),
        };

        let json: serde_json::Value = serde_json::to_value(&config).unwrap();

        assert_eq!(json["isa"], "RV32I");
        assert_eq!(json["vmEnabled"], false);
        assert_eq!(json["pageSize"], 4096);
        let level = &json["hierarchy"][0];
        assert_eq!(level["id"], "l1");
        assert_eq!(level["harvard"], false);
        assert_eq!(level["inclusion"], "NINE");
        assert_eq!(level["busWidth"], 32);
        assert!(level["i"].is_null());
        assert!(level["d"].is_null());
        assert_eq!(level["unified"]["exp"], 10);
        let cache_config = &level["unified"]["config"];
        assert_eq!(cache_config["blockSize"], 64);
        assert_eq!(cache_config["associativity"], 2);
        assert_eq!(cache_config["replacement"], "Random");
        assert_eq!(cache_config["writePolicy"], "write-back");
        assert_eq!(cache_config["writeAllocate"], true);
        assert_eq!(cache_config["hitTime"], 1);
        assert_eq!(cache_config["dirtyPercent"], 0);
        assert_eq!(json["tlb"]["pageWalkStart"], "RAM");
        assert_eq!(json["tlb"]["entries"], 64);
        assert_eq!(json["tlb"]["associativity"], 4);
        assert_eq!(json["tlb"]["replacement"], "LRU");
        assert_eq!(json["tlb"]["latency"], 1);
        assert!(json["tlb"].get("pageSize").is_none());
        assert!(json["tlb"].get("pageTableLevels").is_none());
        assert_eq!(json["ram"]["latency"], 100);
        assert_eq!(json["ram"]["size"], 1024);
        assert_eq!(json["ram"]["dirtyPercent"], 0);
        assert_eq!(json["ram"]["pageTableLevels"], 1);
        assert_eq!(json["ram"]["frameAllocation"], "sequential");
        assert_eq!(json["ram"]["frames"], 4);
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
        assert_eq!(serde_json::to_value(TlbReplacementPolicy::Lfu).unwrap(), "LFU");
        assert_eq!(serde_json::to_value(TlbReplacementPolicy::Plru).unwrap(), "PLRU");
    }

    #[test]
    fn inclusion_policy_variants_match_frontend_strings() {
        assert_eq!(serde_json::to_value(InclusionPolicy::Inclusive).unwrap(), "inclusive");
        assert_eq!(serde_json::to_value(InclusionPolicy::Exclusive).unwrap(), "exclusive");
        assert_eq!(serde_json::to_value(InclusionPolicy::Nine).unwrap(), "NINE");
    }

    #[test]
    fn frame_allocation_policy_variants_match_frontend_strings() {
        assert_eq!(serde_json::to_value(FrameAllocationPolicy::Sequential).unwrap(), "sequential");
        assert_eq!(serde_json::to_value(FrameAllocationPolicy::Random).unwrap(), "random");
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
            page_size: 4096,
            hierarchy: vec![],
            tlb: TlbConfig { page_walk_start: "RAM".into(), entries: 64, associativity: 4, replacement: TlbReplacementPolicy::Lru, latency: 1 },
            ram: RamConfig { latency: 100, size: 1024, dirty_percent: 0, page_table_levels: 1, frame_allocation: FrameAllocationPolicy::Sequential, frames: 4 },
            disk: DiskConfig { latency: 10000 },
            seed: None,
        };

        let json: serde_json::Value = serde_json::to_value(&config).unwrap();
        let obj = json.as_object().expect("HierarchyConfig must serialize to a JSON object");

        let expected_keys = vec!["isa", "vmEnabled", "pageSize", "hierarchy", "tlb", "ram", "disk", "seed"];
        let actual_keys: Vec<&String> = obj.keys().collect();

        assert_eq!(actual_keys.len(), expected_keys.len());
        for key in &expected_keys {
            assert!(obj.contains_key(*key), "Missing expected key: {}", key);
        }
        for key in obj.keys() {
            assert!(expected_keys.contains(&key.as_str()), "Unexpected key: {}", key);
        }
    }
}
