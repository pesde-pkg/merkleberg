use crate::{
  helper::{hash_pospair64, index_height_mmriver, peaks_mmriver},
  merge::Sha256Merge,
  mmriver::{MMRIVER, included_root},
  util::MemStore,
};
use sha2::{Digest, Sha256};

type MemMMRIVER = MMRIVER<[u8; 32], Sha256Merge, MemStore<[u8; 32]>>;

fn leaf_hash(i: u64) -> [u8; 32] {
  let mut hasher = Sha256::new();
  hasher.update(i.to_be_bytes());
  let result = hasher.finalize();
  let mut arr = [0u8; 32];
  arr.copy_from_slice(&result);
  arr
}

fn leaf_index_to_mmr_index(e: u64) -> u64 {
  let mut sum = 0u64;
  let mut e = e;
  while e > 0 {
    let h = 64 - e.leading_zeros();
    sum += (1 << h) - 1;
    let half = 1 << (h - 1);
    e -= half;
  }
  sum
}

async fn build_mmriver_mmr(leaf_count: u64) -> MemMMRIVER {
  let store = MemStore::default();
  let mut mmr = MemMMRIVER::new(0, store);
  
  for e in 0..leaf_count {
    let i = leaf_index_to_mmr_index(e);
    mmr.push(leaf_hash(i)).await.unwrap();
  }
  mmr.commit().await.unwrap();
  mmr
}

#[tokio::test]
async fn test_mmriver_basic() {
  let mmr = build_mmriver_mmr(21).await;
  assert_eq!(mmr.mmr_size(), 39);
  
  let accumulator = mmr.get_accumulator().await.unwrap();
  assert_eq!(accumulator.len(), 3);
}

#[tokio::test]
async fn test_peaks_mmriver() {
  // Test from spec: peaks for MMR(39) should be [30, 37, 38]
  let peaks = peaks_mmriver(38);
  assert_eq!(peaks, vec![30u64, 37, 38]);
  
  // Test other sizes from spec
  assert_eq!(peaks_mmriver(0), vec![0u64]);
  assert_eq!(peaks_mmriver(2), vec![2u64]);
  assert_eq!(peaks_mmriver(3), vec![2u64, 3]);
  assert_eq!(peaks_mmriver(6), vec![6u64]);
  assert_eq!(peaks_mmriver(7), vec![6u64, 7]);
  assert_eq!(peaks_mmriver(9), vec![6u64, 9]);
  assert_eq!(peaks_mmriver(14), vec![14u64]);
}

#[tokio::test]
async fn test_index_height_mmriver() {
  // Test from spec table for MMR(39)
  assert_eq!(index_height_mmriver(0), 0);
  assert_eq!(index_height_mmriver(1), 0);
  assert_eq!(index_height_mmriver(2), 1);
  assert_eq!(index_height_mmriver(3), 0);
  assert_eq!(index_height_mmriver(4), 0);
  assert_eq!(index_height_mmriver(5), 1);
  assert_eq!(index_height_mmriver(6), 2);
  assert_eq!(index_height_mmriver(14), 3);
  assert_eq!(index_height_mmriver(30), 4);
}

#[tokio::test]
async fn test_hash_pospair64() {
  // Test that hash_pospair64 produces correct format
  let left = [1u8; 32];
  let right = [2u8; 32];
  let hash = hash_pospair64(3, &left, &right);
  
  // Should be SHA256(3_be_bytes || left || right)
  let mut hasher = Sha256::new();
  hasher.update(3u64.to_be_bytes());
  hasher.update(left);
  hasher.update(right);
  let expected = hasher.finalize();
  
  let mut expected_arr = [0u8; 32];
  expected_arr.copy_from_slice(&expected);
  assert_eq!(hash, expected_arr);
}

#[tokio::test]
async fn test_mmriver_leaf_values() {
  // Test leaf values match spec - leaves are SHA-256 of their MMR INDEX (i)
  // Leaf at MMR index 0: SHA-256(0)
  let leaf0 = leaf_hash(0);
  let expected0: [u8; 32] = [
    0xaf, 0x55, 0x70, 0xf5, 0xa1, 0x81, 0x0b, 0x7a,
    0xf7, 0x8c, 0xaf, 0x4b, 0xc7, 0x0a, 0x66, 0x0f,
    0x0d, 0xf5, 0x1e, 0x42, 0xba, 0xf9, 0x1d, 0x4d,
    0xe5, 0xb2, 0x32, 0x8d, 0xe0, 0xe8, 0x3d, 0xfc,
  ];
  assert_eq!(leaf0, expected0);
  
  // Leaf at MMR index 1: SHA-256(1)
  let leaf1 = leaf_hash(1);
  let expected1: [u8; 32] = [
    0xcd, 0x26, 0x62, 0x15, 0x4e, 0x6d, 0x76, 0xb2,
    0xb2, 0xb9, 0x2e, 0x70, 0xc0, 0xca, 0xc3, 0xcc,
    0xf5, 0x34, 0xf9, 0xb7, 0x4e, 0xb5, 0xb8, 0x98,
    0x19, 0xec, 0x50, 0x90, 0x83, 0xd0, 0x0a, 0x50,
  ];
  assert_eq!(leaf1, expected1);
  
  // Leaf at MMR index 3 (e=2): SHA-256(3)
  let leaf3 = leaf_hash(3);
  let expected3: [u8; 32] = [
    0xd5, 0x68, 0x8a, 0x52, 0xd5, 0x5a, 0x02, 0xec,
    0x4a, 0xea, 0x5e, 0xc1, 0xea, 0xdf, 0xff, 0xe1,
    0xc9, 0xe0, 0xee, 0x6a, 0x4d, 0xdb, 0xe2, 0x37,
    0x7f, 0x98, 0x32, 0x6d, 0x42, 0xdf, 0xc9, 0x75,
  ];
  assert_eq!(leaf3, expected3);
  
  // Leaf at MMR index 4 (e=3): SHA-256(4)
  let leaf4 = leaf_hash(4);
  let expected4: [u8; 32] = [
    0x80, 0x05, 0xf0, 0x2d, 0x43, 0xfa, 0x06, 0xe7,
    0xd0, 0x58, 0x5f, 0xb6, 0x4c, 0x96, 0x1d, 0x57,
    0xe3, 0x18, 0xb2, 0x7a, 0x14, 0x5c, 0x85, 0x7b,
    0xcd, 0x3a, 0x6b, 0xdb, 0x41, 0x3f, 0xf7, 0xfc,
  ];
  assert_eq!(leaf4, expected4);
  
  // Verify leaf_index_to_mmr_index works correctly
  assert_eq!(leaf_index_to_mmr_index(0), 0);
  assert_eq!(leaf_index_to_mmr_index(1), 1);
  assert_eq!(leaf_index_to_mmr_index(2), 3);
  assert_eq!(leaf_index_to_mmr_index(3), 4);
  assert_eq!(leaf_index_to_mmr_index(4), 7);
  assert_eq!(leaf_index_to_mmr_index(5), 8);
  assert_eq!(leaf_index_to_mmr_index(6), 10);
  assert_eq!(leaf_index_to_mmr_index(20), 38);
}

#[tokio::test]
async fn test_mmriver_node_values() {
  let mmr = build_mmriver_mmr(21).await;
  println!("mmr_size: {}", mmr.mmr_size());
  
  // Node values from spec - verify the MMR builds correctly
  let node0 = mmr.batch().get_elem(0).await.unwrap().unwrap();
  let expected_node0: [u8; 32] = [
    0xaf, 0x55, 0x70, 0xf5, 0xa1, 0x81, 0x0b, 0x7a,
    0xf7, 0x8c, 0xaf, 0x4b, 0xc7, 0x0a, 0x66, 0x0f,
    0x0d, 0xf5, 0x1e, 0x42, 0xba, 0xf9, 0x1d, 0x4d,
    0xe5, 0xb2, 0x32, 0x8d, 0xe0, 0xe8, 0x3d, 0xfc,
  ];
  assert_eq!(node0, expected_node0, "node0 mismatch");
  
  let node1 = mmr.batch().get_elem(1).await.unwrap().unwrap();
  let expected_node1: [u8; 32] = [
    0xcd, 0x26, 0x62, 0x15, 0x4e, 0x6d, 0x76, 0xb2,
    0xb2, 0xb9, 0x2e, 0x70, 0xc0, 0xca, 0xc3, 0xcc,
    0xf5, 0x34, 0xf9, 0xb7, 0x4e, 0xb5, 0xb8, 0x98,
    0x19, 0xec, 0x50, 0x90, 0x83, 0xd0, 0x0a, 0x50,
  ];
  assert_eq!(node1, expected_node1, "node1 mismatch");
  
  let node2 = mmr.batch().get_elem(2).await.unwrap().unwrap();
  let expected_node2: [u8; 32] = [
    0xad, 0x10, 0x40, 0x51, 0xc5, 0x16, 0x81, 0x2e,
    0xa5, 0x87, 0x4c, 0xa3, 0xff, 0x06, 0xd0, 0x25,
    0x83, 0x03, 0x62, 0x3d, 0x04, 0x30, 0x7c, 0x41,
    0xec, 0x80, 0xa7, 0xa1, 0x8b, 0x33, 0x2e, 0xf8,
  ];
  assert_eq!(node2, expected_node2, "node2 mismatch");
  
  let node3 = mmr.batch().get_elem(3).await.unwrap().unwrap();
  let expected_node3: [u8; 32] = [
    0xd5, 0x68, 0x8a, 0x52, 0xd5, 0x5a, 0x02, 0xec,
    0x4a, 0xea, 0x5e, 0xc1, 0xea, 0xdf, 0xff, 0xe1,
    0xc9, 0xe0, 0xee, 0x6a, 0x4d, 0xdb, 0xe2, 0x37,
    0x7f, 0x98, 0x32, 0x6d, 0x42, 0xdf, 0xc9, 0x75,
  ];
  assert_eq!(node3, expected_node3, "node3 mismatch");
  
  let node4 = mmr.batch().get_elem(4).await.unwrap().unwrap();
  let expected_node4: [u8; 32] = [
    0x80, 0x05, 0xf0, 0x2d, 0x43, 0xfa, 0x06, 0xe7,
    0xd0, 0x58, 0x5f, 0xb6, 0x4c, 0x96, 0x1d, 0x57,
    0xe3, 0x18, 0xb2, 0x7a, 0x14, 0x5c, 0x85, 0x7b,
    0xcd, 0x3a, 0x6b, 0xdb, 0x41, 0x3f, 0xf7, 0xfc,
  ];
  assert_eq!(node4, expected_node4, "node4 mismatch");
  
  let node5 = mmr.batch().get_elem(5).await.unwrap().unwrap();
  let expected_node5: [u8; 32] = [
    0x9a, 0x18, 0xd3, 0xbc, 0x0a, 0x7d, 0x50, 0x5e,
    0xf4, 0x5f, 0x98, 0x59, 0x92, 0x27, 0x09, 0x14,
    0xcc, 0x02, 0xb4, 0x4c, 0x91, 0xcc, 0xab, 0xba,
    0x44, 0x8c, 0x54, 0x6a, 0x4b, 0x70, 0xf0, 0xf0,
  ];
  assert_eq!(node5, expected_node5, "node5 mismatch");
  
  let node6 = mmr.batch().get_elem(6).await.unwrap().unwrap();
  let expected_node6: [u8; 32] = [
    0x82, 0x7f, 0x32, 0x13, 0xc1, 0xde, 0x0d, 0x4c,
    0x62, 0x77, 0xca, 0xcc, 0xc1, 0xee, 0xca, 0x32,
    0x5e, 0x45, 0xdf, 0xe2, 0xc6, 0x5a, 0xdc, 0xe1,
    0x94, 0x37, 0x74, 0x21, 0x8d, 0xb6, 0x1f, 0x88,
  ];
  assert_eq!(node6, expected_node6, "node6 mismatch");
}

#[tokio::test]
async fn test_mmriver_accumulator() {
  let mmr = build_mmriver_mmr(21).await;
  let accumulator = mmr.get_accumulator().await.unwrap();
  
  // MMR(39) has peaks at indices [30, 37, 38]
  // Values from spec
  let expected_peak30: [u8; 32] = [
    0xd4, 0xfb, 0x56, 0x49, 0x42, 0x2f, 0xf2, 0xea,
    0xf7, 0xb1, 0xc0, 0xb8, 0x51, 0x58, 0x5a, 0x8c,
    0xfd, 0x14, 0xfb, 0x08, 0xce, 0x11, 0xad, 0xdb,
    0x30, 0x07, 0x5a, 0x96, 0x30, 0x95, 0x82, 0xa7,
  ];
  let expected_peak37: [u8; 32] = [
    0x6a, 0x16, 0x91, 0x05, 0xdc, 0xc4, 0x87, 0xdb,
    0xba, 0xe5, 0x74, 0x7a, 0x0f, 0xd9, 0xb1, 0xd3,
    0x3a, 0x40, 0x32, 0x0c, 0xf9, 0x1c, 0xf9, 0xa3,
    0x23, 0x57, 0x91, 0x39, 0xe7, 0xff, 0x72, 0xaa,
  ];
  let expected_peak38: [u8; 32] = [
    0xe9, 0xa5, 0xf5, 0x20, 0x1e, 0xb3, 0xc3, 0xc8,
    0x56, 0xe0, 0xa2, 0x24, 0x52, 0x7a, 0xf5, 0xac,
    0x7e, 0xb1, 0x76, 0x7f, 0xb1, 0xaf, 0xf9, 0xbd,
    0x53, 0xba, 0x41, 0xa6, 0x0c, 0xde, 0x97, 0x85,
  ];
  
  assert_eq!(accumulator[0], expected_peak30);
  assert_eq!(accumulator[1], expected_peak37);
  assert_eq!(accumulator[2], expected_peak38);
}

#[tokio::test]
async fn test_inclusion_proof_0_in_mmr39() {
  let mmr = build_mmriver_mmr(21).await;
  
  let proof = mmr.gen_inclusion_proof(0).await.unwrap();
  assert_eq!(proof.index(), 0);
  
  let node0 = leaf_hash(0);
  let root = proof.included_root(node0).unwrap();
  
  let accumulator = mmr.get_accumulator().await.unwrap();
  assert_eq!(root, accumulator[0]);
}

#[tokio::test]
async fn test_consistency_proof_mmr7_to_mmr39() {
  // Build MMR(39)
  let mmr = build_mmriver_mmr(21).await;
  
  // MMR(7) has mmr_size = 7 (last node index is 6)
  // Get accumulator for MMR(7)
  let old_mmr = build_mmriver_mmr(4).await; // 4 leaves = MMR size 7
  
  let old_accumulator = old_mmr.get_accumulator().await.unwrap();
  
  // Generate consistency proof from MMR(7) to MMR(39)
  let proof = mmr.gen_consistency_proof(7).await.unwrap();
  assert_eq!(proof.mmr_size_from(), 7);
  assert_eq!(proof.mmr_size_to(), 39);
  
  // Verify consistency
  let new_accumulator = mmr.get_accumulator().await.unwrap();
  let result = proof.verify(old_accumulator, &new_accumulator).unwrap();
  assert!(result);
}

#[tokio::test]
async fn test_consistency_proof_mmr14_to_mmr39() {
  // Build MMR(39)
  let mmr = build_mmriver_mmr(21).await;
  
  // MMR(14) = 8 leaves (mmr_size = 15, last index 14)
  let old_mmr = build_mmriver_mmr(8).await;
  
  let old_accumulator = old_mmr.get_accumulator().await.unwrap();
  
  // Generate consistency proof from MMR(14) to MMR(39)
  let proof = mmr.gen_consistency_proof(15).await.unwrap();
  
  // Verify consistency
  let new_accumulator = mmr.get_accumulator().await.unwrap();
  let result = proof.verify(old_accumulator, &new_accumulator).unwrap();
  assert!(result);
}

#[tokio::test]
async fn test_consistency_proof_mmr30_to_mmr39() {
  // Build MMR(39) = 21 leaves
  let mmr = build_mmriver_mmr(21).await;
  
  // MMR(30) = 16 leaves (mmr_size = 31)
  let old_mmr = build_mmriver_mmr(16).await;
  
  let old_accumulator = old_mmr.get_accumulator().await.unwrap();
  
  // Generate consistency proof from MMR(30) to MMR(39)
  let proof = mmr.gen_consistency_proof(31).await.unwrap();
  
  // Verify consistency
  let new_accumulator = mmr.get_accumulator().await.unwrap();
  let result = proof.verify(old_accumulator, &new_accumulator).unwrap();
  assert!(result);
}

#[tokio::test]
async fn test_included_root_function() {
  let mmr = build_mmriver_mmr(21).await;
  
  let node0 = leaf_hash(0);
  let sibling1 = mmr.batch().get_elem(1).await.unwrap().unwrap();
  let sibling5 = mmr.batch().get_elem(5).await.unwrap().unwrap();
  let sibling13 = mmr.batch().get_elem(13).await.unwrap().unwrap();
  let sibling29 = mmr.batch().get_elem(29).await.unwrap().unwrap();
  
  let proof = vec![sibling1, sibling5, sibling13, sibling29];
  let root = included_root::<Sha256Merge, [u8; 32]>(0, node0, &proof).unwrap();
  
  let expected_peak30: [u8; 32] = [
    0xd4, 0xfb, 0x56, 0x49, 0x42, 0x2f, 0xf2, 0xea,
    0xf7, 0xb1, 0xc0, 0xb8, 0x51, 0x58, 0x5a, 0x8c,
    0xfd, 0x14, 0xfb, 0x08, 0xce, 0x11, 0xad, 0xdb,
    0x30, 0x07, 0x5a, 0x96, 0x30, 0x95, 0x82, 0xa7,
  ];
  assert_eq!(root, expected_peak30);
}