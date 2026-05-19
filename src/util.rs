use crate::collections::BTreeMap;
use crate::merge::Merge;
use crate::mmr_store::{MMRStoreReadOps, MMRStoreWriteOps};
use crate::vec::Vec;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct MemStore<T>(Arc<RwLock<BTreeMap<u64, T>>>);

impl<T> Default for MemStore<T> {
  fn default() -> Self {
    Self::new()
  }
}

impl<T> MemStore<T> {
  pub fn new() -> Self {
    MemStore(Arc::new(RwLock::new(Default::default())))
  }
}

impl<T: Clone + Send + Sync> MMRStoreReadOps<T> for MemStore<T> {
  type Error = core::convert::Infallible;

  async fn get_elem(
    &self,
    pos: u64,
  ) -> core::result::Result<Option<T>, Self::Error> {
    Ok(self.0.read().unwrap().get(&pos).cloned())
  }
}

impl<T: Send + Sync> MMRStoreWriteOps<T> for MemStore<T> {
  type Error = core::convert::Infallible;

  async fn append(
    &mut self,
    pos: u64,
    elems: Vec<T>,
  ) -> core::result::Result<(), Self::Error> {
    let mut store = self.0.write().unwrap();
    for (i, elem) in elems.into_iter().enumerate() {
      store.insert(pos + i as u64, elem);
    }
    Ok(())
  }
}

pub type MemMMR<M> = crate::MMR<M, MemStore<<M as Merge>::Item>>;
