use crate::string::String;

pub type MergeResult<T> = core::result::Result<T, String>;

pub trait Merge {
  type Item;

  fn merge(left: &Self::Item, right: &Self::Item) -> MergeResult<Self::Item>;

  fn merge_peaks(
    peak1: &Self::Item,
    peak2: &Self::Item,
  ) -> MergeResult<Self::Item> {
    Self::merge(peak1, peak2)
  }
}

pub trait MergeMMRIVER {
  type Item: Clone + PartialEq;

  fn merge_pos(
    pos: u64,
    left: &Self::Item,
    right: &Self::Item,
  ) -> MergeResult<Self::Item>;

  fn merge_peaks(
    right: &Self::Item,
    left: &Self::Item,
  ) -> MergeResult<Self::Item>;
}

#[cfg(feature = "sha2")]
use crate::helper::hash_pospair64;

#[cfg(feature = "sha2")]
pub struct Sha256Merge;

#[cfg(feature = "sha2")]
impl MergeMMRIVER for Sha256Merge {
  type Item = [u8; 32];

  fn merge_pos(
    pos: u64,
    left: &Self::Item,
    right: &Self::Item,
  ) -> MergeResult<Self::Item> {
    Ok(hash_pospair64(pos, left, right))
  }

  fn merge_peaks(
    right: &Self::Item,
    left: &Self::Item,
  ) -> MergeResult<Self::Item> {
    Ok(hash_pospair64(0, right, left))
  }
}
