#![cfg_attr(not(feature = "std"), no_std)]

mod error;
pub mod helper;
mod merge;
pub mod mmr;
mod mmr_store;
mod mmriver;
#[cfg(test)]
mod tests;
pub mod util;

pub use error::{Error, Result};
pub use helper::{leaf_index_to_mmr_size, leaf_index_to_pos};
pub use merge::{Merge, MergeResult};
pub use mmr::{InclusionProof, MMR};
pub use mmr_store::{MMRStoreReadOps, MMRStoreWriteOps};
pub use mmriver::{
  ConsistencyProof, InclusionProof as MMRIVERInclusionProof, MMRIVER,
  included_root,
};

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
    use std::string;
  } else {
    extern crate alloc;
    use alloc::borrow;
    use alloc::collections;
    use alloc::vec;
    use alloc::string;
  }
}
