use crate::collections::BTreeMap;
use crate::{MMR, MMRStoreReadOps, MMRStoreWriteOps, Result, vec::Vec};
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

#[async_trait::async_trait]
impl<T: Clone + Send + Sync> MMRStoreReadOps<T> for MemStore<T> {
  async fn get_elem(&self, pos: u64) -> Result<Option<T>> {
    Ok(self.0.read().unwrap().get(&pos).cloned())
  }
}

#[async_trait::async_trait]
impl<T: Send + Sync> MMRStoreWriteOps<T> for MemStore<T> {
  async fn append(&mut self, pos: u64, elems: Vec<T>) -> Result<()> {
    let mut store = self.0.write().unwrap();
    for (i, elem) in elems.into_iter().enumerate() {
      store.insert(pos + i as u64, elem);
    }
    Ok(())
  }
}

pub type MemMMR<T, M> = MMR<T, M, MemStore<T>>;
