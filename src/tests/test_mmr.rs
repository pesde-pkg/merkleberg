use std::convert::Infallible;

use super::{MergeNumberHash, NumberHash};
use crate::Error;
use crate::helper::pos_height_in_tree;
use crate::leaf_index_to_mmr_size;
use crate::merge::Merge;
use crate::mmr::InclusionProof;
use crate::util::{MemMMR, MemStore};
use faster_hex::hex_string;
use proptest::prelude::*;
use rand::{Rng, seq::SliceRandom, thread_rng};

async fn test_mmr(count: u32, proof_elem: Vec<u32>) {
  let store = MemStore::default();
  let mut mmr = MemMMR::<MergeNumberHash>::new(0, store);
  let mut positions: Vec<u64> = Vec::new();
  for i in 0u32..count {
    let pos = mmr.push(&i.to_le_bytes()).await.unwrap();
    positions.push(pos);
  }
  let root = mmr.get_root().await.expect("get root");
  let proof = mmr
    .gen_proof(
      proof_elem
        .iter()
        .map(|elem| positions[*elem as usize])
        .collect(),
    )
    .await
    .expect("gen proof");
  mmr.commit().await.expect("commit changes");
  let result = proof
    .verify(
      root,
      proof_elem
        .iter()
        .map(|elem| {
          (
            positions[*elem as usize],
            MergeNumberHash::leaf_hash(&elem.to_le_bytes()).unwrap(),
          )
        })
        .collect(),
    )
    .unwrap();
  assert!(result);
}

async fn test_gen_new_root_from_proof(count: u32) {
  let store = MemStore::default();
  let mut mmr = MemMMR::<MergeNumberHash>::new(0, store);
  let mut positions: Vec<u64> = Vec::new();
  for i in 0u32..count {
    let pos = mmr.push(&i.to_le_bytes()).await.unwrap();
    positions.push(pos);
  }
  let elem = count - 1;
  let pos = positions[elem as usize];
  let proof = mmr.gen_proof(vec![pos]).await.expect("gen proof");
  let new_elem = count;
  let new_pos = mmr.push(&new_elem.to_le_bytes()).await.unwrap();
  let root = mmr.get_root().await.expect("get root");
  mmr.commit().await.expect("commit changes");
  let calculated_root = proof
    .calculate_root_with_new_leaf(
      vec![(pos, NumberHash::from(elem))],
      new_pos,
      NumberHash::from(new_elem),
      leaf_index_to_mmr_size(new_elem.into()),
    )
    .unwrap();
  assert_eq!(calculated_root, root);
}

#[tokio::test]
async fn test_mmr_root() {
  let store = MemStore::default();
  let mut mmr = MemMMR::<MergeNumberHash>::new(0, store);
  for i in 0u32..11 {
    mmr.push(&i.to_le_bytes()).await.unwrap();
  }
  let root = mmr.get_root().await.expect("get root");
  let hex_root = hex_string(&root.0);
  assert_eq!(
    "f6794677f37a57df6a5ec36ce61036e43a36c1a009d05c81c9aa685dde1fd6e3",
    hex_root
  );
}

#[tokio::test]
async fn test_empty_mmr_root() {
  let store = MemStore::<NumberHash>::default();
  let mmr = MemMMR::<MergeNumberHash>::new(0, store);
  assert_eq!(Err(Error::GetRootOnEmpty), mmr.get_root().await);
}

#[tokio::test]
async fn test_mmr_3_peaks() {
  test_mmr(11, vec![5]).await;
}

#[tokio::test]
async fn test_mmr_2_peaks() {
  test_mmr(10, vec![5]).await;
}

#[tokio::test]
async fn test_mmr_1_peak() {
  test_mmr(8, vec![5]).await;
}

#[tokio::test]
async fn test_mmr_first_elem_proof() {
  test_mmr(11, vec![0]).await;
}

#[tokio::test]
async fn test_mmr_last_elem_proof() {
  test_mmr(11, vec![10]).await;
}

#[tokio::test]
async fn test_mmr_1_elem() {
  test_mmr(1, vec![0]).await;
}

#[tokio::test]
async fn test_mmr_2_elems() {
  test_mmr(2, vec![0]).await;
  test_mmr(2, vec![1]).await;
}

#[tokio::test]
async fn test_mmr_2_leaves_merkle_proof() {
  test_mmr(11, vec![3, 7]).await;
  test_mmr(11, vec![3, 4]).await;
}

#[tokio::test]
async fn test_mmr_2_sibling_leaves_merkle_proof() {
  test_mmr(11, vec![4, 5]).await;
  test_mmr(11, vec![5, 6]).await;
  test_mmr(11, vec![6, 7]).await;
}

#[tokio::test]
async fn test_mmr_3_leaves_merkle_proof() {
  test_mmr(11, vec![4, 5, 6]).await;
  test_mmr(11, vec![3, 5, 7]).await;
  test_mmr(11, vec![3, 4, 5]).await;
  test_mmr(100, vec![3, 5, 13]).await;
}

#[tokio::test]
async fn test_gen_root_from_proof() {
  test_gen_new_root_from_proof(11).await;
}

#[tokio::test]
async fn test_gen_proof_with_duplicate_leaves() {
  test_mmr(10, vec![5, 5]).await;
}

async fn test_invalid_proof_verification(
  leaf_count: u32,
  positions_to_verify: Vec<u64>,
  tampered_positions: Vec<usize>,
  handrolled_proof_positions: Option<Vec<u64>>,
) {
  #[derive(Clone, PartialEq)]
  enum MyItem {
    Number(u32),
    Merged(Box<MyItem>, Box<MyItem>),
  }

  impl std::fmt::Debug for MyItem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
      match self {
        MyItem::Number(x) => f.write_fmt(format_args!("{}", x)),
        MyItem::Merged(a, b) => {
          f.write_fmt(format_args!("Merged({:#?}, {:#?})", a, b))
        }
      }
    }
  }

  #[derive(Debug)]
  struct MyMerge;

  impl Merge for MyMerge {
    type Item = MyItem;
    type Error = Infallible;

    fn leaf_hash(data: &[u8]) -> Result<Self::Item, Self::Error> {
      let num = data
        .get(..4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or(0);
      Ok(MyItem::Number(num))
    }

    fn merge_pos(
      _pos: u64,
      lhs: &Self::Item,
      rhs: &Self::Item,
    ) -> Result<Self::Item, Self::Error> {
      Ok(MyItem::Merged(Box::new(lhs.clone()), Box::new(rhs.clone())))
    }
  }

  let store = MemStore::default();
  let mut mmr = MemMMR::<MyMerge>::new(0, store);
  let mut positions: Vec<u64> = Vec::new();
  for i in 0u32..leaf_count {
    let pos = mmr.push(&i.to_le_bytes()).await.unwrap();
    positions.push(pos);
  }
  let root = mmr.get_root().await.unwrap();

  let mut entries_to_verify: Vec<(u64, MyItem)> = Vec::new();
  for pos in &positions_to_verify {
    let elem = mmr.batch().get_elem(*pos).await.unwrap().unwrap();
    entries_to_verify.push((*pos, elem));
  }

  let mut tampered_entries_to_verify = entries_to_verify.clone();
  tampered_positions.iter().for_each(|proof_pos| {
    tampered_entries_to_verify[*proof_pos] = (
      tampered_entries_to_verify[*proof_pos].0,
      MyItem::Number(31337),
    )
  });

  let handrolled_proof: Option<InclusionProof<MyMerge>> =
    if let Some(handrolled_proof_positions) = handrolled_proof_positions {
      let mut proof_elems: Vec<MyItem> = Vec::new();
      for pos in &handrolled_proof_positions {
        let elem = mmr.batch().get_elem(*pos).await.unwrap().unwrap();
        proof_elems.push(elem);
      }
      Some(InclusionProof::new(mmr.mmr_size(), proof_elems))
    } else {
      None
    };

  if let Some(handrolled_proof) = handrolled_proof {
    let handrolled_proof_result =
      handrolled_proof.verify(root.clone(), tampered_entries_to_verify.clone());
    assert!(
      handrolled_proof_result.is_err() || !handrolled_proof_result.unwrap()
    );
  }

  match mmr.gen_proof(positions_to_verify.clone()).await {
    Ok(proof) => {
      assert!(proof.verify(root.clone(), entries_to_verify).unwrap());
      assert!(!proof.verify(root, tampered_entries_to_verify).unwrap());
    }
    Err(Error::NodeProofsNotSupported) => {
      assert!(
        positions_to_verify
          .iter()
          .any(|pos| pos_height_in_tree(*pos) > 0)
      );
    }
    Err(e) => panic!("Unexpected error: {}", e),
  }
}

#[tokio::test]
async fn test_generic_proofs() {
  test_invalid_proof_verification(7, vec![5], vec![0], Some(vec![2, 9, 10]))
    .await;
  test_invalid_proof_verification(7, vec![1, 2], vec![0], Some(vec![5, 9, 10]))
    .await;
  test_invalid_proof_verification(7, vec![1, 5], vec![0], Some(vec![0, 9, 10]))
    .await;
  test_invalid_proof_verification(
    7,
    vec![1, 6],
    vec![0],
    Some(vec![0, 5, 9, 10]),
  )
  .await;
  test_invalid_proof_verification(7, vec![5, 6], vec![0], Some(vec![2, 9, 10]))
    .await;
  test_invalid_proof_verification(
    7,
    vec![1, 5, 6],
    vec![0],
    Some(vec![0, 9, 10]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![1, 5, 7],
    vec![0],
    Some(vec![0, 8, 10]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![5, 6, 7],
    vec![0],
    Some(vec![2, 8, 10]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![5, 6, 7, 8, 9, 10],
    vec![0],
    Some(vec![2]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![1, 5, 7, 8, 9, 10],
    vec![0],
    Some(vec![0]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![0, 1, 5, 7, 8, 9, 10],
    vec![0],
    Some(vec![]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![0, 1, 5, 6, 7, 8, 9, 10],
    vec![0],
    Some(vec![]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![0, 1, 2, 5, 6, 7, 8, 9, 10],
    vec![0],
    Some(vec![]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![0, 1, 2, 3, 7, 8, 9, 10],
    vec![0],
    Some(vec![4]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![0, 2, 3, 7, 8, 9, 10],
    vec![0],
    Some(vec![1, 4]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![0, 3, 7, 8, 9, 10],
    vec![0],
    Some(vec![1, 4]),
  )
  .await;
  test_invalid_proof_verification(
    7,
    vec![0, 2, 3, 7, 8, 9, 10],
    vec![0],
    Some(vec![1, 4]),
  )
  .await;
}

proptest! {
    #[test]
    fn test_random_mmr(count in 10u32..500u32) {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut leaves: Vec<u32> = (0..count).collect();
            let mut rng = thread_rng();
            leaves.shuffle(&mut rng);
            let leaves_count = rng.gen_range(1..count - 1);
            leaves.truncate(leaves_count as usize);
            test_mmr(count, leaves).await;
        });
    }

    #[test]
    fn test_random_gen_root_with_new_leaf(count in 1u32..500u32) {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            test_gen_new_root_from_proof(count).await;
        });
    }
}
