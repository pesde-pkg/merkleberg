use crate::string::String;

pub type MergeResult<T> = core::result::Result<T, String>;

pub trait Merge {
  type Item: Clone + PartialEq;

  fn merge_pos(
    pos: u64,
    left: &Self::Item,
    right: &Self::Item,
  ) -> MergeResult<Self::Item>;

  fn merge(left: &Self::Item, right: &Self::Item) -> MergeResult<Self::Item> {
    Self::merge_pos(0, left, right)
  }

  fn merge_peaks(
    left: &Self::Item,
    right: &Self::Item,
  ) -> MergeResult<Self::Item> {
    Self::merge(left, right)
  }
}

#[cfg(feature = "sha2")]
use crate::helper::hash_pospair64;

#[cfg(feature = "sha2")]
pub struct Sha256Merge;

#[cfg(feature = "sha2")]
impl Merge for Sha256Merge {
  type Item = [u8; 32];

  fn merge_pos(
    pos: u64,
    left: &Self::Item,
    right: &Self::Item,
  ) -> MergeResult<Self::Item> {
    Ok(hash_pospair64(pos, left, right))
  }
}
