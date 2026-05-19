use super::MergeNumberHash;
use crate::{
  MMR, PeaksIter,
  helper::{get_peak_map, pos_height_in_tree},
  leaf_index_to_mmr_size, leaf_index_to_pos,
  util::MemStore,
};
use proptest::prelude::*;
use std::sync::OnceLock;

/// Positions of 0..100_000 elem
fn build_index_to_pos() -> Vec<u64> {
  let rt = tokio::runtime::Runtime::new().unwrap();
  rt.block_on(async {
    let store = MemStore::default();
    let mut mmr = MMR::<MergeNumberHash, _>::new(0, store);
    let mut positions = Vec::new();
    for i in 0u32..100_000 {
      let pos = mmr.push(&i.to_le_bytes()).await.unwrap();
      positions.push(pos);
    }
    positions
  })
}

/// mmr size when 0..100_000 elem
fn build_index_to_mmr_size() -> Vec<u64> {
  let rt = tokio::runtime::Runtime::new().unwrap();
  rt.block_on(async {
    let store = MemStore::default();
    let mut mmr = MMR::<MergeNumberHash, _>::new(0, store);
    let mut sizes = Vec::new();
    for i in 0u32..100_000 {
      mmr.push(&i.to_le_bytes()).await.unwrap();
      sizes.push(mmr.mmr_size());
    }
    sizes
  })
}

static INDEX_TO_POS: OnceLock<Vec<u64>> = OnceLock::new();
static INDEX_TO_MMR_SIZE: OnceLock<Vec<u64>> = OnceLock::new();

fn get_index_to_pos() -> &'static Vec<u64> {
  INDEX_TO_POS.get_or_init(build_index_to_pos)
}

fn get_index_to_mmr_size() -> &'static Vec<u64> {
  INDEX_TO_MMR_SIZE.get_or_init(build_index_to_mmr_size)
}

#[test]
fn test_leaf_index_to_pos() {
  assert_eq!(leaf_index_to_pos(0), 0);
  assert_eq!(leaf_index_to_pos(1), 1);
  assert_eq!(leaf_index_to_pos(2), 3);
}

#[test]
fn test_leaf_index_to_mmr_size() {
  assert_eq!(leaf_index_to_mmr_size(0), 1);
  assert_eq!(leaf_index_to_mmr_size(1), 3);
  assert_eq!(leaf_index_to_mmr_size(2), 4);
}

#[test]
fn test_pos_height_in_tree() {
  assert_eq!(pos_height_in_tree(0), 0);
  assert_eq!(pos_height_in_tree(1), 0);
  assert_eq!(pos_height_in_tree(2), 1);
  assert_eq!(pos_height_in_tree(3), 0);
  assert_eq!(pos_height_in_tree(4), 0);
  assert_eq!(pos_height_in_tree(6), 2);
  assert_eq!(pos_height_in_tree(7), 0);
}

#[test]
fn test_get_peak_map() {
  assert_eq!(get_peak_map(0), 0b0);
  assert_eq!(get_peak_map(1), 0b1);
  assert_eq!(get_peak_map(3), 0b10);
  assert_eq!(get_peak_map(4), 0b11);
  // 5 and 6 are not valid mmr_size, it will return the bitmap of the last valid mmr (size 4)
  assert_eq!(get_peak_map(5), 0b11);
  assert_eq!(get_peak_map(6), 0b11);
  assert_eq!(get_peak_map(7), 0b100);
  assert_eq!(get_peak_map(8), 0b101);
  // 9 is not valid mmr_size, it will return the bitmap of the last valid mmr (size 8)
  assert_eq!(get_peak_map(9), 0b101);
  assert_eq!(get_peak_map(15), 0b1000);
  assert_eq!(get_peak_map(16), 0b1001);
  assert_eq!(get_peak_map(18), 0b1010);
  assert_eq!(get_peak_map(19), 0b1011);
}

#[test]
fn test_peaks_iter() {
  assert_eq!(PeaksIter::new(0).collect::<Vec<_>>(), Vec::<u64>::new());
  assert_eq!(PeaksIter::new(1).collect::<Vec<_>>(), vec![0]);
  assert_eq!(PeaksIter::new(3).collect::<Vec<_>>(), vec![2]);
  assert_eq!(PeaksIter::new(4).collect::<Vec<_>>(), vec![2, 3]);
  // 5 and 6 are not valid mmr_size, it will return the peaks of the last valid mmr (size 4)
  assert_eq!(PeaksIter::new(5).collect::<Vec<_>>(), vec![2, 3]);
  assert_eq!(PeaksIter::new(6).collect::<Vec<_>>(), vec![2, 3]);
  assert_eq!(PeaksIter::new(7).collect::<Vec<_>>(), vec![6]);
  assert_eq!(PeaksIter::new(8).collect::<Vec<_>>(), vec![6, 7]);
  // 9 is not valid mmr_size, it will return the peaks of the last valid mmr (size 8)
  assert_eq!(PeaksIter::new(9).collect::<Vec<_>>(), vec![6, 7]);
  assert_eq!(PeaksIter::new(15).collect::<Vec<_>>(), vec![14]);
  assert_eq!(PeaksIter::new(16).collect::<Vec<_>>(), vec![14, 15]);
  assert_eq!(PeaksIter::new(18).collect::<Vec<_>>(), vec![14, 17]);
  assert_eq!(PeaksIter::new(19).collect::<Vec<_>>(), vec![14, 17, 18]);
}

proptest! {
    #[test]
    fn test_leaf_index_to_pos_randomly(index in 0..100_000usize) {
        let pos = leaf_index_to_pos(index as u64);
        assert_eq!(pos, get_index_to_pos()[index]);
    }

    #[test]
    fn test_leaf_index_to_mmr_size_randomly(index in 0..100_000usize) {
        assert_eq!(leaf_index_to_mmr_size(index as u64), get_index_to_mmr_size()[index]);
    }
}
