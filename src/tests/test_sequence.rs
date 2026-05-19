use std::fmt;

use proptest::proptest;
use rand::{prelude::*, thread_rng};

use crate::{MMR, Merge, MergeResult, util::MemStore};

#[derive(Eq, PartialEq, Clone, Default)]
struct NumberRange {
  start: u32,
  end: u32,
}

struct MergeNumberRange;

impl fmt::Debug for NumberRange {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "NumberRange({}, {})", self.start, self.end)
  }
}

impl fmt::Debug for MergeNumberRange {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "MergeNumberRange")
  }
}

impl From<u32> for NumberRange {
  fn from(num: u32) -> Self {
    Self {
      start: num,
      end: num,
    }
  }
}

impl NumberRange {
  fn is_normalized(&self) -> bool {
    self.start <= self.end
  }
}

impl Merge for MergeNumberRange {
  type Item = NumberRange;

  fn leaf_hash(data: &[u8]) -> MergeResult<Self::Item> {
    let num = data
      .get(..4)
      .and_then(|b| b.try_into().ok())
      .map(u32::from_le_bytes)
      .unwrap_or(0);
    Ok(Self::Item {
      start: num,
      end: num,
    })
  }

  fn merge_pos(
    _pos: u64,
    lhs: &Self::Item,
    rhs: &Self::Item,
  ) -> MergeResult<Self::Item> {
    Ok(Self::Item {
      start: lhs.start,
      end: rhs.end,
    })
  }

  fn merge_peaks(
    left: &Self::Item,
    right: &Self::Item,
  ) -> MergeResult<Self::Item> {
    Self::merge_pos(0, right, left)
  }
}

async fn test_sequence_sub_func(count: u32, proof_elem: Vec<u32>) {
  let store = MemStore::default();
  let mut mmr = MMR::<MergeNumberRange, _>::new(0, store);
  let mut positions: Vec<u64> = Vec::new();
  for i in 0..count {
    let pos = mmr.push(&i.to_le_bytes()).await.expect("push");
    positions.push(pos);
  }
  let root = mmr.get_root().await.expect("get_root");
  assert!(root.is_normalized());
  let proof = mmr
    .gen_proof(
      proof_elem
        .iter()
        .map(|elem| positions[*elem as usize])
        .collect(),
    )
    .await
    .expect("gen_proof");
  for item in proof.proof_items() {
    assert!(item.is_normalized())
  }
  mmr.commit().await.expect("commit");
  let result = proof
    .verify(
      root,
      proof_elem
        .iter()
        .map(|elem| {
          (
            positions[*elem as usize],
            MergeNumberRange::leaf_hash(&elem.to_le_bytes()).unwrap(),
          )
        })
        .collect(),
    )
    .expect("verify");
  assert!(result);
}

proptest! {
    #[test]
    fn test_sequence(count in 10u32..500u32) {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut leaves: Vec<u32> = (0..count).collect();
            let mut rng = thread_rng();
            leaves.shuffle(&mut rng);
            let leaves_count = rng.gen_range(1..count - 1);
            leaves.truncate(leaves_count as usize);
            test_sequence_sub_func(count, leaves).await;
        });
    }
}
