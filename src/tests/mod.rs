mod test_accumulate_headers;
mod test_helper;
mod test_incremental;
mod test_mmr;
mod test_mmriver;
mod test_sequence;

use std::convert::Infallible;

use crate::Merge;
use blake2b_rs::{Blake2b, Blake2bBuilder};
use bytes::Bytes;

fn new_blake2b() -> Blake2b {
  Blake2bBuilder::new(32).build()
}

#[derive(Eq, PartialEq, Clone, Debug, Default)]
struct NumberHash(pub Bytes);
impl From<u32> for NumberHash {
  fn from(num: u32) -> Self {
    let mut hasher = new_blake2b();
    let mut hash = [0u8; 32];
    hasher.update(&num.to_le_bytes());
    hasher.finalize(&mut hash);
    NumberHash(hash.to_vec().into())
  }
}

struct MergeNumberHash;

impl Merge for MergeNumberHash {
  type Item = NumberHash;
  type Error = Infallible;

  fn leaf_hash(data: &[u8]) -> Result<Self::Item, Self::Error> {
    let mut hasher = new_blake2b();
    let mut hash = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut hash);
    Ok(NumberHash(hash.to_vec().into()))
  }

  fn merge_pos(
    _pos: u64,
    left: &Self::Item,
    right: &Self::Item,
  ) -> Result<Self::Item, Self::Error> {
    let mut hasher = new_blake2b();
    let mut hash = [0u8; 32];
    hasher.update(&left.0);
    hasher.update(&right.0);
    hasher.finalize(&mut hash);
    Ok(NumberHash(hash.to_vec().into()))
  }
}
