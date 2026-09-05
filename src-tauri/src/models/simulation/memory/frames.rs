use std::collections::HashMap;

use super::backing_store::DataStore;
use super::config::FrameAllocationPolicy;
use super::seeded_random;

#[derive(Debug, Clone, Copy)]
struct FrameMeta {
    vpn: Option<u64>,
    dirty: bool,
    last_used: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapOut {
    pub frame: u64,
    pub vpn: u64,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSnapshot {
    pub frame: u64,
    pub vpn: Option<u64>,
    pub valid: bool,
    pub dirty: bool,
}

/// The physical side of virtual memory, a fixed pool of frames sized from the configured RAM.
/// Any page can go in any frame, with plain LRU eviction.
pub struct FrameTable {
    page_size: u64,
    num_frames: u64,
    bytes: DataStore,
    meta: HashMap<u64, FrameMeta>,
    tick: u64,
    allocation: FrameAllocationPolicy,
    seed: u64,
    next_position: u64,
    random_attempt: u64,
}

impl FrameTable {
    pub fn new(page_size: u32, ram_size: u32, allocation: FrameAllocationPolicy, seed: u64) -> Self {
        let page_size = page_size.max(1) as u64;
        let num_frames = ((ram_size.max(1) as u64) / page_size).max(1);
        Self {
            page_size,
            num_frames,
            // Small blocks the size of a word, so a frame can tell a really written word from one that just defaults to zero.
            bytes: DataStore::new(4),
            meta: HashMap::new(),
            tick: 0,
            allocation,
            seed,
            next_position: 0,
            random_attempt: 0,
        }
    }

    pub fn page_size(&self) -> u64 {
        self.page_size
    }

    pub fn num_frames(&self) -> u64 {
        self.num_frames
    }

    fn touch(&mut self, frame: u64) {
        self.tick += 1;
        let tick = self.tick;
        if let Some(m) = self.meta.get_mut(&frame) {
            m.last_used = tick;
        }
    }

    /// Picks an empty frame if there is room, otherwise the least recently used one.
    /// Returns whoever used to be there, if anyone, so the caller can write them back to disk first.
    pub fn allocate(&mut self, vpn: u64) -> (u64, Option<SwapOut>) {
        self.tick += 1;
        let tick = self.tick;

        if (self.meta.len() as u64) < self.num_frames {
            let frame = self.find_free_frame();
            self.meta.insert(frame, FrameMeta { vpn: Some(vpn), dirty: false, last_used: tick });
            return (frame, None);
        }

        let victim = *self
            .meta
            .iter()
            .min_by_key(|(_, m)| m.last_used)
            .map(|(f, _)| f)
            .expect("num_frames is always > 0, so a full table always has a victim");
        let old = self.meta.remove(&victim).expect("victim came from meta.iter()");
        let swap_out = old.vpn.map(|old_vpn| SwapOut { frame: victim, vpn: old_vpn, dirty: old.dirty });
        self.meta.insert(victim, FrameMeta { vpn: Some(vpn), dirty: false, last_used: tick });
        (victim, swap_out)
    }

    /// Only called when a frame is free, so this always finds one.
    /// Sequential remembers where it left off, Random picks a starting frame from the seed and moves forward from there.
    fn find_free_frame(&mut self) -> u64 {
        let mut candidate = match self.allocation {
            FrameAllocationPolicy::Sequential => self.next_position % self.num_frames,
            FrameAllocationPolicy::Random => {
                self.random_attempt += 1;
                seeded_random::value(self.seed, &[9, self.random_attempt]) % self.num_frames
            }
        };
        while self.meta.contains_key(&candidate) {
            candidate = (candidate + 1) % self.num_frames;
        }
        self.next_position = (candidate + 1) % self.num_frames;
        candidate
    }

    fn frame_address(&self, frame: u64, offset: u64) -> u64 {
        frame * self.page_size + offset
    }

    pub fn read_byte(&mut self, frame: u64, offset: u64) -> u8 {
        self.touch(frame);
        self.bytes.read_byte(self.frame_address(frame, offset))
    }

    pub fn write_byte(&mut self, frame: u64, offset: u64, value: u8) {
        self.touch(frame);
        if let Some(m) = self.meta.get_mut(&frame) {
            m.dirty = true;
        }
        self.bytes.write_byte(self.frame_address(frame, offset), value);
    }

    /// Loading a page in from disk is not a modification, so this never marks the frame dirty.
    /// A word that was never really written stays untouched here too, instead of getting a fake zero.
    pub fn write_page_bytes(&mut self, frame: u64, bytes: &[u8], populated: &[bool]) {
        for (word_index, &is_populated) in populated.iter().enumerate() {
            if !is_populated {
                continue;
            }
            let base = word_index * 4;
            for (i, &b) in bytes.iter().enumerate().skip(base).take(4) {
                self.bytes.write_byte(self.frame_address(frame, i as u64), b);
            }
        }
    }

    /// Reads bytes starting at a physical address, even across a frame boundary.
    /// Used by the cache inspector, which only knows the address, not which frame it belongs to.
    pub fn read_bytes(&self, address: u64, len: usize) -> Vec<u8> {
        self.bytes.read_block(address, len)
    }

    pub fn read_page_bytes(&self, frame: u64) -> Vec<u8> {
        self.bytes.read_block(frame * self.page_size, self.page_size as usize)
    }

    /// One flag per word of this frame, whether that word was ever actually written.
    /// A word that reads as zero just because nothing wrote it comes back false here.
    pub fn read_page_populated(&self, frame: u64) -> Vec<bool> {
        let words = (self.page_size / 4).max(1);
        (0..words).map(|w| self.bytes.is_populated(frame * self.page_size + w * 4)).collect()
    }

    pub fn snapshot(&self, frame: u64) -> FrameSnapshot {
        match self.meta.get(&frame) {
            Some(m) => FrameSnapshot { frame, vpn: m.vpn, valid: true, dirty: m.dirty },
            None => FrameSnapshot { frame, vpn: None, valid: false, dirty: false },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_size_divided_by_page_size_gives_the_frame_count() {
        let table = FrameTable::new(4096, 4096 * 4, FrameAllocationPolicy::Sequential, 1);
        assert_eq!(table.num_frames(), 4);
    }

    #[test]
    fn a_fresh_table_reports_every_frame_as_invalid() {
        let table = FrameTable::new(64, 64 * 2, FrameAllocationPolicy::Sequential, 1);
        assert!(!table.snapshot(0).valid);
        assert!(!table.snapshot(1).valid);
    }

    #[test]
    fn allocate_fills_empty_frames_first_without_evicting_anyone() {
        let mut table = FrameTable::new(64, 64 * 2, FrameAllocationPolicy::Sequential, 1);
        let (frame_a, evicted_a) = table.allocate(10);
        let (frame_b, evicted_b) = table.allocate(20);
        assert!(evicted_a.is_none());
        assert!(evicted_b.is_none());
        assert_ne!(frame_a, frame_b);
        assert_eq!(table.snapshot(frame_a).vpn, Some(10));
        assert_eq!(table.snapshot(frame_b).vpn, Some(20));
    }

    #[test]
    fn allocating_past_capacity_evicts_the_least_recently_used_frame() {
        let mut table = FrameTable::new(64, 64 * 2, FrameAllocationPolicy::Sequential, 1); // 2 frames
        let (frame_a, _) = table.allocate(10);
        let (frame_b, _) = table.allocate(20);
        table.read_byte(frame_a, 0); // touch frame_a so frame_b becomes the least recently used

        let (frame_c, evicted) = table.allocate(30);
        assert_eq!(frame_c, frame_b, "the least recently used frame must be the one reused");
        let evicted = evicted.expect("the table was full, so allocate must report who got evicted");
        assert_eq!(evicted.vpn, 20);
        assert!(!evicted.dirty);
        assert_eq!(table.snapshot(frame_a).vpn, Some(10), "the touched frame must survive");
    }

    #[test]
    fn write_byte_marks_the_frame_dirty_and_read_byte_does_not() {
        let mut table = FrameTable::new(64, 64 * 2, FrameAllocationPolicy::Sequential, 1);
        let (frame, _) = table.allocate(10);
        assert!(!table.snapshot(frame).dirty);
        table.read_byte(frame, 0);
        assert!(!table.snapshot(frame).dirty, "a read must never mark a frame dirty");
        table.write_byte(frame, 0, 0xAB);
        assert!(table.snapshot(frame).dirty);
    }

    #[test]
    fn eviction_reports_whether_the_evicted_frame_was_dirty() {
        let mut table = FrameTable::new(64, 64, FrameAllocationPolicy::Sequential, 1); // 1 frame
        let (frame, _) = table.allocate(10);
        table.write_byte(frame, 0, 0xAB);
        let (_, evicted) = table.allocate(20);
        assert!(evicted.unwrap().dirty);
    }

    #[test]
    fn write_page_bytes_does_not_mark_the_frame_dirty() {
        let mut table = FrameTable::new(4, 4 * 2, FrameAllocationPolicy::Sequential, 1);
        let (frame, _) = table.allocate(10);
        table.write_page_bytes(frame, &[1, 2, 3, 4], &[true]);
        assert!(!table.snapshot(frame).dirty, "loading a page in from disk is not a modification");
        assert_eq!(table.read_page_bytes(frame), vec![1, 2, 3, 4]);
    }

    #[test]
    fn read_byte_reflects_bytes_written_by_write_page_bytes() {
        let mut table = FrameTable::new(4, 4, FrameAllocationPolicy::Sequential, 1);
        let (frame, _) = table.allocate(1);
        table.write_page_bytes(frame, &[9, 8, 7, 6], &[true]);
        assert_eq!(table.read_byte(frame, 2), 7);
    }

    #[test]
    fn a_fresh_frame_reports_every_word_as_unpopulated() {
        let mut table = FrameTable::new(4, 4 * 2, FrameAllocationPolicy::Sequential, 1); // page size 4 gives 1 word per page
        let (frame, _) = table.allocate(1);
        assert_eq!(table.read_page_populated(frame), vec![false]);
    }

    #[test]
    fn write_page_bytes_only_marks_the_words_the_source_actually_had_populated() {
        let mut table = FrameTable::new(8, 8 * 2, FrameAllocationPolicy::Sequential, 1); // page size 8 gives 2 words per page
        let (frame, _) = table.allocate(1);
        // Only the second word is populated on the source side, the first word's bytes are just the default zero fill.
        table.write_page_bytes(frame, &[0, 0, 0, 0, 9, 8, 7, 6], &[false, true]);
        assert_eq!(table.read_page_populated(frame), vec![false, true], "the unpopulated word must not become populated just because it was zero");
        assert_eq!(table.read_byte(frame, 4), 9, "the populated word's real bytes must still be written");
    }

    #[test]
    fn sequential_allocation_fills_frames_in_order_and_remembers_where_it_left_off() {
        let mut table = FrameTable::new(4, 4 * 4, FrameAllocationPolicy::Sequential, 1); // 4 frames
        let (a, _) = table.allocate(10);
        let (b, _) = table.allocate(20);
        let (c, _) = table.allocate(30);
        assert_eq!((a, b, c), (0, 1, 2), "sequential must fill frames 0, 1, 2 in order");
    }

    #[test]
    fn sequential_allocation_skips_over_a_frame_that_is_still_occupied() {
        let mut table = FrameTable::new(4, 4 * 3, FrameAllocationPolicy::Sequential, 1); // 3 frames
        table.allocate(10); // frame 0
        let (frame_b, _) = table.allocate(20); // frame 1
        // Evict frame 1 specifically by filling the table then touching frame 0 and the new frame so frame 1 stays untouched.
        let (frame_c, _) = table.allocate(30); // frame 2, table now full
        table.read_byte(0, 0);
        table.read_byte(frame_c, 0);
        let (_, evicted) = table.allocate(40); // must evict frame_b, the untouched one
        assert_eq!(evicted.unwrap().frame, frame_b);

        // This mostly checks that allocate never panics or reuses a frame that is still occupied.
        table.allocate(50);
    }

    #[test]
    fn random_allocation_is_deterministic_for_the_same_seed() {
        let mut a = FrameTable::new(4, 4 * 8, FrameAllocationPolicy::Random, 42);
        let mut b = FrameTable::new(4, 4 * 8, FrameAllocationPolicy::Random, 42);
        let sequence_a: Vec<u64> = (0..8).map(|i| a.allocate(i).0).collect();
        let sequence_b: Vec<u64> = (0..8).map(|i| b.allocate(i).0).collect();
        assert_eq!(sequence_a, sequence_b, "the same seed must pick the same frames in the same order");
    }

    #[test]
    fn random_allocation_still_fills_every_frame_exactly_once_before_evicting() {
        let mut table = FrameTable::new(4, 4 * 8, FrameAllocationPolicy::Random, 7); // 8 frames
        let mut seen = std::collections::HashSet::new();
        for i in 0..8u64 {
            let (frame, evicted) = table.allocate(i);
            assert!(evicted.is_none(), "must never evict while a free frame still exists");
            assert!(seen.insert(frame), "must never reuse a frame that's still occupied");
        }
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn a_different_seed_gives_a_different_random_allocation_order() {
        let mut a = FrameTable::new(4, 4 * 8, FrameAllocationPolicy::Random, 1);
        let mut b = FrameTable::new(4, 4 * 8, FrameAllocationPolicy::Random, 2);
        let sequence_a: Vec<u64> = (0..8).map(|i| a.allocate(i).0).collect();
        let sequence_b: Vec<u64> = (0..8).map(|i| b.allocate(i).0).collect();
        assert_ne!(sequence_a, sequence_b, "different seeds should not reliably pick the identical order");
    }
}
