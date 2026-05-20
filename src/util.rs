//! Utility types for MMR storage.
//!
//! Provides [`MemStore`] for in-memory storage, suitable for testing and
//! small datasets.

use crate::collections::BTreeMap;
use crate::merge::Merge;
use crate::mmr_store::{MMRStoreReadOps, MMRStoreWriteOps};
use crate::vec::Vec;
use core::convert::Infallible;
use std::sync::{Arc, RwLock};

/// In-memory storage backend for MMR.
///
/// Simple `BTreeMap`-based store using `RwLock` for thread safety.
/// Suitable for testing and small datasets.
///
/// ## Cloning
///
/// `MemStore` is cloneable, enabling MMR instances to share storage.
#[derive(Clone)]
pub struct MemStore<T>(Arc<RwLock<BTreeMap<u64, T>>>);

impl<T> Default for MemStore<T> {
  fn default() -> Self {
    Self::new()
  }
}

impl<T> MemStore<T> {
  /// Create empty store.
  pub fn new() -> Self {
    MemStore(Arc::new(RwLock::new(Default::default())))
  }
}

impl<T: Clone + Send + Sync> MMRStoreReadOps<T> for MemStore<T> {
  type Error = Infallible;

  async fn get_elem(&self, pos: u64) -> Result<Option<T>, Self::Error> {
    Ok(self.0.read().unwrap().get(&pos).cloned())
  }

  async fn get_elems(
    &self,
    positions: impl Iterator<Item = u64> + Send,
  ) -> Result<Vec<Option<T>>, Self::Error> {
    let store = self.0.read().unwrap();
    Ok(positions.map(|pos| store.get(&pos).cloned()).collect())
  }
}

impl<T: Send + Sync> MMRStoreWriteOps<T> for MemStore<T> {
  type Error = Infallible;

  async fn append(
    &mut self,
    pos: u64,
    elems: Vec<T>,
  ) -> Result<(), Self::Error> {
    let mut store = self.0.write().unwrap();
    for (i, elem) in elems.into_iter().enumerate() {
      store.insert(pos + i as u64, elem);
    }
    Ok(())
  }
}

/// Type alias for MMR with in-memory store.
///
/// ```rust,ignore
/// use merkleberg::{util::MemMMR, DigestMerge};
/// use sha2::Sha256;
///
/// let mmr: MemMMR<DigestMerge<Sha256>> = MMR::new(0, MemStore::default());
/// ```
pub type MemMMR<M> = crate::MMR<M, MemStore<<M as Merge>::Item>>;

/// Type alias for MMRIVER with in-memory store.
///
/// ```rust,ignore
/// use merkleberg::{util::MemMMRIVER, DigestMerge};
/// use sha2::Sha256;
///
/// let mmr: MemMMRIVER<DigestMerge<Sha256>> = MMR::new(0, MemStore::default());
/// ```
pub type MemMMRIVER<M> = crate::MMRIVER<M, MemStore<<M as Merge>::Item>>;
