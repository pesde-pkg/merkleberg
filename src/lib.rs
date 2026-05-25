#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(doc, doc = include_str!("../README.md"))]

mod error;
pub mod helper;
mod merge;
pub mod mmr;
mod mmr_store;
pub mod mmriver;
#[cfg(test)]
mod tests;
pub mod util;

pub use error::{Error, Result, UserError};
pub use helper::{
  PeaksIter, PeaksMMRIVERIter, leaf_index_to_mmr_size, leaf_index_to_pos,
};
pub use merge::Merge;
pub use mmr::MMR;
pub use mmr_store::{MMRStoreReadOps, MMRStoreWriteOps};
pub use mmriver::{MMRIVER, included_root};

#[cfg(feature = "digest")]
pub use merge::DigestMerge;

#[cfg(feature = "unsafe-digest")]
#[allow(deprecated)]
pub use merge::DigestMergeUnsafe;

cfg_if::cfg_if! {
  if #[cfg(feature = "std")] {
    use std::borrow;
    use std::collections;
    use std::vec;
  } else {
    extern crate alloc;
    use alloc::borrow;
    use alloc::collections;
    use alloc::vec;
    use alloc::string;
  }
}
