#![cfg_attr(not(feature = "std"), no_std)]

mod error;
pub mod helper;
mod merge;
mod mmr;
mod mmr_store;
mod mmriver;
#[cfg(test)]
mod tests;
pub mod util;

pub use error::{Error, Result};
#[cfg(feature = "sha2")]
pub use helper::hash_pospair64;
pub use helper::{leaf_index_to_mmr_size, leaf_index_to_pos};
#[cfg(feature = "sha2")]
pub use merge::Sha256Merge;
pub use merge::{Merge, MergeMMRIVER};
pub use mmr::{MMR, MerkleProof};
pub use mmr_store::{MMRStoreReadOps, MMRStoreWriteOps};
pub use mmriver::{ConsistencyProof, InclusionProof, MMRIVER, included_root};

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
