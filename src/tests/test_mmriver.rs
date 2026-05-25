cfg_if::cfg_if! {
  if #[cfg(all(feature = "std", not(feature = "unsafe-digest")))] {
    use super::MergeNumberHash;
    use crate::{
      helper::PeaksMMRIVERIter,
      MMRIVER,
      merge::Merge,
      util::MemStore,
    };

    #[tokio::test]
    async fn test_mmriver_basic() {
      let store = MemStore::default();
      let mut mmr: MMRIVER<MergeNumberHash, _> = MMRIVER::new(0, store);

      for i in 0u64..21 {
        mmr.push(&i.to_be_bytes()).await.unwrap();
      }
      mmr.commit().await.unwrap();

      assert_eq!(mmr.mmr_size(), 39);

      let accumulator = mmr.get_accumulator().await.unwrap();
      assert_eq!(accumulator.len(), 3);
    }

    #[tokio::test]
    async fn test_peaks_mmriver_iter() {
      let peaks: Vec<u64> = PeaksMMRIVERIter::new(38).collect();
      assert_eq!(peaks, vec![30u64, 37, 38]);

      assert_eq!(PeaksMMRIVERIter::new(0).collect::<Vec<_>>(), vec![0u64]);
      assert_eq!(PeaksMMRIVERIter::new(2).collect::<Vec<_>>(), vec![2u64]);
      assert_eq!(PeaksMMRIVERIter::new(3).collect::<Vec<_>>(), vec![2u64, 3]);
      assert_eq!(PeaksMMRIVERIter::new(6).collect::<Vec<_>>(), vec![6u64]);
      assert_eq!(PeaksMMRIVERIter::new(7).collect::<Vec<_>>(), vec![6u64, 7]);
      assert_eq!(PeaksMMRIVERIter::new(9).collect::<Vec<_>>(), vec![6u64, 9]);
      assert_eq!(PeaksMMRIVERIter::new(14).collect::<Vec<_>>(), vec![14u64]);
    }

    #[tokio::test]
    async fn test_mmriver_inclusion_proof() {
      let store = MemStore::default();
      let mut mmr: MMRIVER<MergeNumberHash, _> = MMRIVER::new(0, store);

      for i in 0u64..21 {
        mmr.push(&i.to_be_bytes()).await.unwrap();
      }
      mmr.commit().await.unwrap();

      let accumulator = mmr.get_accumulator().await.unwrap();

      let proof = mmr.gen_inclusion_proof(0).await.unwrap();
      assert_eq!(proof.index(), 0);

      let leaf_hash = MergeNumberHash::leaf_hash(&0u64.to_be_bytes()).unwrap();
      assert!(proof.verify(leaf_hash, &accumulator).unwrap());
    }

    #[tokio::test]
    async fn test_mmriver_consistency_proof() {
      let store = MemStore::default();
      let mut mmr: MMRIVER<MergeNumberHash, _> = MMRIVER::new(0, store);

      for i in 0u64..21 {
        mmr.push(&i.to_be_bytes()).await.unwrap();
      }
      mmr.commit().await.unwrap();

      let old_store = MemStore::default();
      let mut old_mmr: MMRIVER<MergeNumberHash, _> = MMRIVER::new(0, old_store);

      for i in 0u64..4 {
        old_mmr.push(&i.to_be_bytes()).await.unwrap();
      }
      old_mmr.commit().await.unwrap();

      let old_accumulator = old_mmr.get_accumulator().await.unwrap();
      let proof = mmr.gen_consistency_proof(7).await.unwrap();

      assert_eq!(proof.mmr_size_from(), 7);
      assert_eq!(proof.mmr_size_to(), 39);

      let new_accumulator = mmr.get_accumulator().await.unwrap();
      assert!(proof.verify(old_accumulator, &new_accumulator).unwrap());
    }
  }
}

cfg_if::cfg_if! {
  if #[cfg(feature = "unsafe-digest")] {
    use crate::{
      helper::index_height_mmriver,
      mmriver::{MMRIVER, included_root},
      util::MemStore,
    };
    #[allow(deprecated)]
    use crate::merge::DigestMergeUnsafe;
    use digest::Output;
    use sha2::{Digest as _, Sha256};

    #[allow(deprecated)]
    type SpecMMRIVER = MMRIVER<DigestMergeUnsafe<Sha256>, MemStore<Output<Sha256>>>;

    fn leaf_hash_spec(i: u64) -> Output<Sha256> {
      let mut hasher = Sha256::new();
      hasher.update(i.to_be_bytes());
      hasher.finalize()
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

    async fn build_spec_mmriver(leaf_count: u64) -> SpecMMRIVER {
      let store = MemStore::default();
      let mut mmr = SpecMMRIVER::new(0, store);

      for e in 0..leaf_count {
        let i = leaf_index_to_mmr_index(e);
        mmr.push(&i.to_be_bytes()).await.unwrap();
      }
      mmr.commit().await.unwrap();
      mmr
    }

    fn to_fixed_32(arr: Output<Sha256>) -> [u8; 32] {
      let mut fixed = [0u8; 32];
      fixed.copy_from_slice(&arr);
      fixed
    }

    #[tokio::test]
    async fn test_index_height_mmriver() {
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
    async fn test_mmriver_leaf_values_spec() {
      // Spec test vectors: SHA256(i) without domain prefix
      // Leaf at MMR index 0: SHA256(0)
      let leaf0 = leaf_hash_spec(0);
      let expected0: [u8; 32] = [
        0xaf, 0x55, 0x70, 0xf5, 0xa1, 0x81, 0x0b, 0x7a, 0xf7, 0x8c, 0xaf, 0x4b,
        0xc7, 0x0a, 0x66, 0x0f, 0x0d, 0xf5, 0x1e, 0x42, 0xba, 0xf9, 0x1d, 0x4d,
        0xe5, 0xb2, 0x32, 0x8d, 0xe0, 0xe8, 0x3d, 0xfc,
      ];
      assert_eq!(to_fixed_32(leaf0), expected0);

      // Leaf at MMR index 1: SHA256(1)
      let leaf1 = leaf_hash_spec(1);
      let expected1: [u8; 32] = [
        0xcd, 0x26, 0x62, 0x15, 0x4e, 0x6d, 0x76, 0xb2, 0xb2, 0xb9, 0x2e, 0x70,
        0xc0, 0xca, 0xc3, 0xcc, 0xf5, 0x34, 0xf9, 0xb7, 0x4e, 0xb5, 0xb8, 0x98,
        0x19, 0xec, 0x50, 0x90, 0x83, 0xd0, 0x0a, 0x50,
      ];
      assert_eq!(to_fixed_32(leaf1), expected1);

      // Leaf at MMR index 3: SHA256(3)
      let leaf3 = leaf_hash_spec(3);
      let expected3: [u8; 32] = [
        0xd5, 0x68, 0x8a, 0x52, 0xd5, 0x5a, 0x02, 0xec, 0x4a, 0xea, 0x5e, 0xc1,
        0xea, 0xdf, 0xff, 0xe1, 0xc9, 0xe0, 0xee, 0x6a, 0x4d, 0xdb, 0xe2, 0x37,
        0x7f, 0x98, 0x32, 0x6d, 0x42, 0xdf, 0xc9, 0x75,
      ];
      assert_eq!(to_fixed_32(leaf3), expected3);

      // Verify leaf_index_to_mmr_index
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
    async fn test_mmriver_node_values_spec() {
      let mmr = build_spec_mmriver(21).await;
      assert_eq!(mmr.mmr_size(), 39);

      // Verify leaves at positions 0, 1, 3, 4 match their spec leaf_hash values
      let node0 = mmr.batch().get_elem(0).await.unwrap().unwrap();
      assert_eq!(node0, leaf_hash_spec(0));

      let node1 = mmr.batch().get_elem(1).await.unwrap().unwrap();
      assert_eq!(node1, leaf_hash_spec(1));

      // Position 2 is a node (parent of 0 and 1)
      let node2 = mmr.batch().get_elem(2).await.unwrap().unwrap();
      assert_ne!(node2, leaf_hash_spec(0));
      assert_ne!(node2, leaf_hash_spec(1));

      // Verify position 3 and 4 are leaves
      let node3 = mmr.batch().get_elem(3).await.unwrap().unwrap();
      assert_eq!(node3, leaf_hash_spec(3));

      let node4 = mmr.batch().get_elem(4).await.unwrap().unwrap();
      assert_eq!(node4, leaf_hash_spec(4));
    }

    #[tokio::test]
    async fn test_mmriver_accumulator_spec() {
      let mmr = build_spec_mmriver(21).await;
      let accumulator = mmr.get_accumulator().await.unwrap();

      // MMR(39) has peaks at indices [30, 37, 38]
      assert_eq!(accumulator.len(), 3);

      // Values from spec
      let expected_peak30: [u8; 32] = [
        0xd4, 0xfb, 0x56, 0x49, 0x42, 0x2f, 0xf2, 0xea, 0xf7, 0xb1, 0xc0, 0xb8,
        0x51, 0x58, 0x5a, 0x8c, 0xfd, 0x14, 0xfb, 0x08, 0xce, 0x11, 0xad, 0xdb,
        0x30, 0x07, 0x5a, 0x96, 0x30, 0x95, 0x82, 0xa7,
      ];
      assert_eq!(to_fixed_32(accumulator[0]), expected_peak30);

      let expected_peak37: [u8; 32] = [
        0x6a, 0x16, 0x91, 0x05, 0xdc, 0xc4, 0x87, 0xdb, 0xba, 0xe5, 0x74, 0x7a,
        0x0f, 0xd9, 0xb1, 0xd3, 0x3a, 0x40, 0x32, 0x0c, 0xf9, 0x1c, 0xf9, 0xa3,
        0x23, 0x57, 0x91, 0x39, 0xe7, 0xff, 0x72, 0xaa,
      ];
      assert_eq!(to_fixed_32(accumulator[1]), expected_peak37);

      let expected_peak38: [u8; 32] = [
        0xe9, 0xa5, 0xf5, 0x20, 0x1e, 0xb3, 0xc3, 0xc8, 0x56, 0xe0, 0xa2, 0x24,
        0x52, 0x7a, 0xf5, 0xac, 0x7e, 0xb1, 0x76, 0x7f, 0xb1, 0xaf, 0xf9, 0xbd,
        0x53, 0xba, 0x41, 0xa6, 0x0c, 0xde, 0x97, 0x85,
      ];
      assert_eq!(to_fixed_32(accumulator[2]), expected_peak38);
    }

    #[tokio::test]
    async fn test_inclusion_proof_0_in_mmr39() {
      let mmr = build_spec_mmriver(21).await;

      let proof = mmr.gen_inclusion_proof(0).await.unwrap();
      assert_eq!(proof.index(), 0);

      let node0 = leaf_hash_spec(0);
      let root = proof.included_root(node0).unwrap();

      let accumulator = mmr.get_accumulator().await.unwrap();
      assert_eq!(root, accumulator[0]);
    }

    #[tokio::test]
    async fn test_consistency_proof_mmr7_to_mmr39() {
      let mmr = build_spec_mmriver(21).await;
      let old_mmr = build_spec_mmriver(4).await;

      let old_accumulator = old_mmr.get_accumulator().await.unwrap();
      let proof = mmr.gen_consistency_proof(7).await.unwrap();

      assert_eq!(proof.mmr_size_from(), 7);
      assert_eq!(proof.mmr_size_to(), 39);

      let new_accumulator = mmr.get_accumulator().await.unwrap();
      let result = proof.verify(&old_accumulator, &new_accumulator).unwrap();
      assert!(result);
    }

    #[tokio::test]
    async fn test_consistency_proof_mmr14_to_mmr39() {
      let mmr = build_spec_mmriver(21).await;
      let old_mmr = build_spec_mmriver(8).await;

      let old_accumulator = old_mmr.get_accumulator().await.unwrap();
      let proof = mmr.gen_consistency_proof(15).await.unwrap();

      let new_accumulator = mmr.get_accumulator().await.unwrap();
      let result = proof.verify(&old_accumulator, &new_accumulator).unwrap();
      assert!(result);
    }

    #[tokio::test]
    async fn test_consistency_proof_mmr31_to_mmr39() {
      let mmr = build_spec_mmriver(21).await;
      let old_mmr = build_spec_mmriver(16).await;

      let old_accumulator = old_mmr.get_accumulator().await.unwrap();
      let proof = mmr.gen_consistency_proof(31).await.unwrap();

      let new_accumulator = mmr.get_accumulator().await.unwrap();
      let result = proof.verify(&old_accumulator, &new_accumulator).unwrap();
      assert!(result);
    }

    #[tokio::test]
    async fn test_included_root_function() {
      let mmr = build_spec_mmriver(21).await;

      let node0 = leaf_hash_spec(0);
      let sibling1 = mmr.batch().get_elem(1).await.unwrap().unwrap();
      let sibling5 = mmr.batch().get_elem(5).await.unwrap().unwrap();
      let sibling13 = mmr.batch().get_elem(13).await.unwrap().unwrap();
      let sibling29 = mmr.batch().get_elem(29).await.unwrap().unwrap();

      let proof = vec![sibling1, sibling5, sibling13, sibling29];
      #[allow(deprecated)]
      let root = included_root::<DigestMergeUnsafe<Sha256>>(0, node0, &proof).unwrap();

      let peak30 = mmr.batch().get_elem(30).await.unwrap().unwrap();
      assert_eq!(root, peak30);
    }
  }
}

cfg_if::cfg_if! {
  if #[cfg(all(feature = "digest", not(feature = "unsafe-digest")))] {
    use crate::merge::DigestMerge;
    use digest::Output;
    use sha2::{Digest, Sha256};

    #[tokio::test]
    async fn test_domain_separation_leaf_prefix() {
      let data = 42u32.to_le_bytes();
      let secure_hash = DigestMerge::<Sha256>::leaf_hash(&data).unwrap();

      // Verify domain prefix changes output
      let mut hasher = Sha256::new();
      hasher.update(data);
      let expected_without_prefix = hasher.finalize();

      assert_ne!(secure_hash, expected_without_prefix);
    }

    #[tokio::test]
    async fn test_domain_separation_node_prefix() {
      let left = [1u8; 32];
      let right = [2u8; 32];
      let pos = 3u64;

      let secure_hash = DigestMerge::<Sha256>::merge_pos(pos, &Output::<Sha256>::from(left), &Output::<Sha256>::from(right)).unwrap();

      // Verify domain prefix changes output
      let mut hasher = Sha256::new();
      hasher.update(pos.to_be_bytes());
      hasher.update(left);
      hasher.update(right);
      let expected_without_prefix = hasher.finalize();

      assert_ne!(secure_hash, expected_without_prefix);
    }

    #[tokio::test]
    async fn test_domain_separation_no_collision() {
      // Craft a leaf value
      let leaf_data = 123u64.to_be_bytes();
      let leaf_hash = DigestMerge::<Sha256>::leaf_hash(&leaf_data).unwrap();

      // Try to make it match a node hash by Copy (should fail due to domain prefix)
      let fake_left = leaf_hash;
      let fake_right = leaf_hash;
      let node_hash = DigestMerge::<Sha256>::merge_pos(0, &fake_left, &fake_right).unwrap();

      // They must differ even if we try to match them
      assert_ne!(leaf_hash, node_hash);
    }
  }
}
