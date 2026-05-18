#![cfg_attr(not(feature = "std"), no_std)]

mod error;
pub mod helper;
mod merge;
mod mmr;
mod mmriver;
mod mmr_store;
#[cfg(test)]
mod tests;
pub mod util;

pub use error::{Error, Result};
pub use helper::{
  consistency_proof_paths, hash_pospair64, inclusion_proof_path, leaf_index_to_mmr_size,
  leaf_index_to_pos,
};
pub use merge::{Merge, MergeMMRIVER, Sha256Merge};
pub use mmr::{MMR, MerkleProof};
pub use mmriver::{ConsistencyProof, InclusionProof, MMRIVER, included_root};
pub use mmr_store::{MMRStoreReadOps, MMRStoreWriteOps};

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
