use std::collections::HashMap;

use super::seeded_random;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTableEntry {
    pub points_to_or_ppn: u64,
    pub present: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageTableEntrySnapshot {
    pub level: u8,
    pub index: u64,
    /// The entry's real storage key, needed to ask for the table this entry points to.
    /// The index alone is not enough, since it repeats across different branches.
    pub key: u64,
    pub points_to_or_ppn: u64,
    pub present: bool,
}

/// An entry only exists once some walk has actually touched it, nothing needs to be evicted to make room.
/// Levels are numbered starting from 1 at the leaf up to the root.
pub struct PageTable {
    levels: u8,
    /// How many address bits each level uses, from the leaf first to the root last.
    level_bits: Vec<u32>,
    seed: u64,
    entries: HashMap<(u8, u64), PageTableEntry>,
}

impl PageTable {
    /// Splits the address bits evenly across the levels.
    /// Any leftover bits go to the leaf first, so the leaf ends up with as many or more bits than the root.
    fn distribute_index_bits(total_vpn_bits: u32, levels: u8) -> Vec<u32> {
        let levels = levels.max(1) as u32;
        let base = total_vpn_bits / levels;
        let remainder = total_vpn_bits % levels;
        (0..levels).map(|i| if i < remainder { base + 1 } else { base }).collect()
    }

    /// address_bits is the full address width for the ISA.
    /// The bits left after the page offset are the ones split across the levels.
    pub fn new(levels: u8, page_size: u32, address_bits: u32, seed: u64) -> Self {
        let offset_bits = page_size.max(1).trailing_zeros();
        let total_vpn_bits = address_bits.saturating_sub(offset_bits);
        Self::with_level_bits(Self::distribute_index_bits(total_vpn_bits, levels), seed)
    }

    /// Builds a page table with a specific bit width for each level, instead of splitting them evenly.
    pub fn with_level_bits(level_bits: Vec<u32>, seed: u64) -> Self {
        let levels = (level_bits.len().max(1)) as u8;
        Self { levels, level_bits, seed, entries: HashMap::new() }
    }

    pub fn levels(&self) -> u8 {
        self.levels
    }

    pub fn level_bits(&self) -> &[u32] {
        &self.level_bits
    }

    /// Bits consumed by every level below this one, nothing is below level 1.
    fn bits_below(&self, level: u8) -> u32 {
        self.level_bits.iter().take(level.saturating_sub(1) as usize).sum()
    }

    /// The mask that keeps only the bits this level owns.
    fn mask_for_level(&self, level: u8) -> u64 {
        let bits = self.level_bits.get(level.saturating_sub(1) as usize).copied().unwrap_or(0);
        if bits == 0 { 0 } else if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 }
    }

    /// Keeps the higher bits too, not just the ones this level owns, so two pages sharing only this level's low bits stay distinct.
    /// This is what actually gets used to store an entry.
    fn key_prefix_at_level(&self, vpn: u64, level: u8) -> u64 {
        let shift = self.bits_below(level);
        if shift >= 64 {
            // Every real bit was already consumed by lower levels.
            return 0;
        }
        vpn >> shift
    }

    /// The page number masked down to just this level's own bits.
    /// Use this for display or the made up address, never as a storage key.
    fn prefix_at_level(&self, vpn: u64, level: u8) -> u64 {
        self.key_prefix_at_level(vpn, level) & self.mask_for_level(level)
    }

    /// A stable made up address for this level and page, only so a cache can match a lookup the same way it matches real data.
    /// Never a real physical address.
    pub fn synthetic_address(&self, level: u8, vpn: u64) -> u64 {
        seeded_random::value(self.seed, &[6, level as u64, self.prefix_at_level(vpn, level)])
    }

    /// Marks that this branch of the walk has been reached before, and keeps that mark forever.
    /// The value stored here does not matter, only whether the branch exists.
    pub fn resolve_intermediate(&mut self, level: u8, vpn: u64) -> PageTableEntry {
        let key = (level, self.key_prefix_at_level(vpn, level));
        *self.entries.entry(key).or_insert_with(|| PageTableEntry { points_to_or_ppn: 0, present: true })
    }

    /// None means this page was never mapped, present false means it used to be resident but got swapped out.
    pub fn leaf_entry(&self, vpn: u64) -> Option<PageTableEntry> {
        self.entries.get(&(1, self.key_prefix_at_level(vpn, 1))).copied()
    }

    pub fn set_leaf_present(&mut self, vpn: u64, ppn: u64) {
        self.entries.insert((1, self.key_prefix_at_level(vpn, 1)), PageTableEntry { points_to_or_ppn: ppn, present: true });
    }

    pub fn set_leaf_absent(&mut self, vpn: u64) {
        if let Some(entry) = self.entries.get_mut(&(1, self.key_prefix_at_level(vpn, 1))) {
            entry.present = false;
        }
    }

    /// Returns one table's worth of entries, sorted by index. This is the root table if no parent key is given, otherwise the table it points to.
    /// Only entries within the requested range are returned, the same paging convention other views use.
    pub fn entries_under(&self, level: u8, parent_key: Option<u64>, start_index: u64, count: u64) -> Vec<PageTableEntrySnapshot> {
        let end = start_index.saturating_add(count);
        let mask = self.mask_for_level(level);
        let own_bits = self.level_bits.get(level.saturating_sub(1) as usize).copied().unwrap_or(0);
        let mut rows: Vec<PageTableEntrySnapshot> = self
            .entries
            .iter()
            .filter(|((l, key), _)| {
                if *l != level {
                    return false;
                }
                let belongs_to_parent = match parent_key {
                    None => true, // the root level has exactly one table
                    Some(parent) => key >> own_bits == parent,
                };
                let index = key & mask;
                belongs_to_parent && index >= start_index && index < end
            })
            .map(|((l, key), e)| PageTableEntrySnapshot { level: *l, index: key & mask, key: *key, points_to_or_ppn: e.points_to_or_ppn, present: e.present })
            .collect();
        rows.sort_by_key(|r| r.index);
        rows
    }

    pub fn entry_count_for_level(&self, level: u8) -> usize {
        self.entries.keys().filter(|(l, _)| *l == level).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_page_table_has_no_leaf_entry_for_any_vpn() {
        let table = PageTable::new(2, 4096, 32, 1);
        assert!(table.leaf_entry(5).is_none());
    }

    #[test]
    fn set_leaf_present_then_leaf_entry_reflects_it() {
        let mut table = PageTable::new(2, 4096, 32, 1);
        table.set_leaf_present(5, 7);
        let entry = table.leaf_entry(5).unwrap();
        assert_eq!(entry.points_to_or_ppn, 7);
        assert!(entry.present);
    }

    #[test]
    fn set_leaf_absent_keeps_the_entry_but_marks_it_not_present() {
        let mut table = PageTable::new(2, 4096, 32, 1);
        table.set_leaf_present(5, 7);
        table.set_leaf_absent(5);
        let entry = table.leaf_entry(5).unwrap();
        assert!(!entry.present, "the entry must still exist, since it used to be mapped");
    }

    #[test]
    fn resolve_intermediate_is_deterministic_and_lazy() {
        let mut table = PageTable::new(2, 4096, 32, 1);
        assert_eq!(table.entry_count_for_level(2), 0);
        let first = table.resolve_intermediate(2, 5);
        assert_eq!(table.entry_count_for_level(2), 1);
        let second = table.resolve_intermediate(2, 5);
        assert_eq!(first, second, "resolving the same (level, vpn) twice must not create a second entry or change the value");
    }

    #[test]
    fn intermediate_entries_always_resolve_to_a_zero_points_to_or_ppn_regardless_of_seed() {
        // Intermediate levels have no real address to show, so this is always 0 no matter the seed.
        let mut a = PageTable::new(2, 4096, 32, 1);
        let mut b = PageTable::new(2, 4096, 32, 2);
        assert_eq!(a.resolve_intermediate(2, 5).points_to_or_ppn, 0);
        assert_eq!(b.resolve_intermediate(2, 5).points_to_or_ppn, 0);
    }

    #[test]
    fn synthetic_address_is_stable_for_the_same_level_and_vpn() {
        let table = PageTable::new(2, 4096, 32, 1);
        assert_eq!(table.synthetic_address(1, 9), table.synthetic_address(1, 9));
    }

    #[test]
    fn two_vpns_sharing_the_same_higher_level_prefix_get_the_same_synthetic_address_at_that_level() {
        // These two pages share every bit except the lowest 10, so they land in the same entry at level 2 but not level 1.
        let table = PageTable::new(2, 4096, 32, 1);
        assert_eq!(table.synthetic_address(2, 0), table.synthetic_address(2, 1));
        assert_ne!(table.synthetic_address(1, 0), table.synthetic_address(1, 1), "but at the leaf level they're different entries");
    }

    #[test]
    fn level_bits_returns_the_configured_widths() {
        let table = PageTable::with_level_bits(vec![6, 6, 5], 1);
        assert_eq!(table.level_bits(), &[6, 6, 5]);
    }

    #[test]
    fn entries_under_reports_each_entrys_real_unmasked_key() {
        let mut table = PageTable::with_level_bits(vec![4, 4], 1);
        table.set_leaf_present(20, 70); // key 20 (0b10100), masked index 20 & 15 = 4
        let rows = table.entries_under(1, Some(1), 0, 100);
        assert_eq!(rows[0].key, 20);
        assert_eq!(rows[0].index, 4);
    }

    #[test]
    fn entries_under_filters_to_only_the_children_of_the_given_parent() {
        // With 2 levels of 4 bits each, leaf keys with top bits equal to 0 belong to parent 0, and key 20 belongs to parent 1.
        let mut table = PageTable::with_level_bits(vec![4, 4], 1);
        table.set_leaf_present(3, 30);
        table.set_leaf_present(5, 50);
        table.set_leaf_present(20, 70);

        let under_parent_0 = table.entries_under(1, Some(0), 0, 100);
        assert_eq!(under_parent_0.len(), 2);
        assert!(under_parent_0.iter().all(|r| r.points_to_or_ppn == 30 || r.points_to_or_ppn == 50));

        let under_parent_1 = table.entries_under(1, Some(1), 0, 100);
        assert_eq!(under_parent_1.len(), 1);
        assert_eq!(under_parent_1[0].points_to_or_ppn, 70);
    }

    #[test]
    fn entries_under_the_root_ignores_parent_key_since_there_is_only_one_root_table() {
        let mut table = PageTable::with_level_bits(vec![4, 4], 1);
        table.resolve_intermediate(2, 3);
        table.resolve_intermediate(2, 20);
        let rows = table.entries_under(2, None, 0, 100);
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn entries_under_only_returns_entries_whose_display_index_falls_in_the_requested_range() {
        let mut table = PageTable::with_level_bits(vec![4, 4], 1);
        table.set_leaf_present(0, 10);
        table.set_leaf_present(5, 50);
        table.set_leaf_present(9, 90);

        let window = table.entries_under(1, Some(0), 0, 6); // indices [0, 6)
        assert_eq!(window.iter().map(|r| r.index).collect::<Vec<_>>(), vec![0, 5]);
    }

    #[test]
    fn entries_under_a_parent_that_was_never_touched_is_empty() {
        let table = PageTable::with_level_bits(vec![4, 4], 1);
        assert!(table.entries_under(1, Some(99), 0, 100).is_empty());
    }

    #[test]
    fn leaf_entries_for_vpns_sharing_this_levels_low_bits_do_not_alias_across_branches() {
        // These two pages happen to show the same leaf index once masked for display, but must still be stored separately.
        let mut table = PageTable::new(2, 4096, 32, 1);
        let vpn_text = 0x00400000u64 / 4096;
        let vpn_data = 0x10000000u64 / 4096;
        assert_eq!(vpn_text % 1024, 0);
        assert_eq!(vpn_data % 1024, 0, "both share the same masked leaf index, which is the point of this test");

        table.set_leaf_present(vpn_text, 7);
        table.set_leaf_present(vpn_data, 42);

        assert_eq!(table.leaf_entry(vpn_text).unwrap().points_to_or_ppn, 7, "the data page's mapping must not have overwritten text's");
        assert_eq!(table.leaf_entry(vpn_data).unwrap().points_to_or_ppn, 42);
    }

    #[test]
    fn entries_under_only_returns_entries_touched_at_that_level() {
        let mut table = PageTable::new(3, 4096, 32, 1);
        table.resolve_intermediate(2, 0);
        table.resolve_intermediate(2, 5000); // a different prefix at level 2, same parent
        table.resolve_intermediate(3, 0); // a different level, must not show up
        let rows = table.entries_under(2, Some(0), 0, 1000);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.level == 2));
    }

    #[test]
    fn entries_under_returns_every_touched_entry_sorted_by_index() {
        let mut table = PageTable::new(2, 4096, 32, 1);
        for vpn in [3000u64, 1000, 2000] {
            table.resolve_intermediate(2, vpn);
        }
        // These three pages resolve to indices 2, 0, and 1 at the root level, so the sorted list comes back as 0, 1, 2.
        let all = table.entries_under(2, None, 0, 1000);
        assert_eq!(all.iter().map(|r| r.index).collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn levels_returns_the_configured_count() {
        let table = PageTable::new(3, 4096, 32, 1);
        assert_eq!(table.levels(), 3);
    }

    #[test]
    fn distribute_index_bits_splits_evenly_and_puts_the_remainder_on_the_leaf_side() {
        assert_eq!(PageTable::distribute_index_bits(20, 1), vec![20]);
        assert_eq!(PageTable::distribute_index_bits(20, 2), vec![10, 10]);
        assert_eq!(PageTable::distribute_index_bits(22, 4), vec![6, 6, 5, 5]);
    }

    #[test]
    fn a_levels_index_is_masked_to_its_own_bit_width_not_just_shifted() {
        // A page with a bit belonging to level 2 must not leak into level 1's index.
        let mut table = PageTable::new(2, 4096, 32, 1);
        let vpn = 1u64 << 10;
        table.set_leaf_present(vpn, 99);
        let leaf_indices: Vec<u64> = table.entries_under(1, Some(1), 0, 2000).iter().map(|e| e.index).collect();
        assert_eq!(leaf_indices, vec![0]);
    }

    #[test]
    fn uneven_leftover_bits_land_on_the_leafs_index_range() {
        // 20 bits over 3 levels split as 7, 7, 6 favoring the leaf.
        // A page using the leaf's full range must resolve to its top value, not get clipped.
        let mut table = PageTable::new(3, 4096, 32, 1);
        let vpn: u64 = 0b111_1111;
        table.set_leaf_present(vpn, 1);
        let leaf_indices: Vec<u64> = table.entries_under(1, Some(0), 0, 300).iter().map(|e| e.index).collect();
        assert_eq!(leaf_indices, vec![127]);
    }
}
