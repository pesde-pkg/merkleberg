use proptest::proptest;

use super::{MergeNumberHash, NumberHash};
use crate::merge::Merge as _;
use crate::util::{MemMMR, MemStore};

proptest! {
    #[test]
    fn test_incremental(start in 1u32..500, steps in 1usize..50, turns in 10usize..20) {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            test_incremental_with_params(start, steps, turns).await;
        });
    }
}

async fn test_incremental_with_params(start: u32, steps: usize, turns: usize) {
  let store = MemStore::default();
  let mut mmr = MemMMR::<MergeNumberHash>::new(0, store);

  let mut curr: u32 = 0;

  let mut positions: Vec<u64> = Vec::new();
  for _ in 0u32..start {
    let pos = mmr.push(&curr.to_le_bytes()).await.unwrap();
    curr += 1;
    positions.push(pos);
  }
  mmr.commit().await.expect("commit changes");

  for turn in 0..turns {
    let prev_root = mmr.get_root().await.expect("get root");
    let mut new_positions: Vec<u64> = Vec::new();
    let mut leaves: Vec<NumberHash> = Vec::new();
    for _ in 0..steps {
      let leaf = MergeNumberHash::leaf_hash(&curr.to_le_bytes()).unwrap();
      let pos = mmr.push(&curr.to_le_bytes()).await.unwrap();
      curr += 1;
      new_positions.push(pos);
      leaves.push(leaf);
    }
    mmr.commit().await.expect("commit changes");
    let proof = mmr.gen_proof(new_positions).await.expect("gen proof");
    let root = mmr.get_root().await.expect("get root");
    let result = proof.verify_incremental(&root, &prev_root, leaves).unwrap();
    assert!(
      result,
      "start: {start}, steps: {steps}, turn: {turn}, curr: {curr}"
    );
  }
}
