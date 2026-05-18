use crate::vec::Vec;
use core::error::Error;
use core::future::Future;
use core::result::Result;

pub struct MMRBatch<Elem, Store> {
  memory_batch: Vec<(u64, Vec<Elem>)>,
  store: Store,
}

impl<Elem, Store> MMRBatch<Elem, Store> {
  pub fn new(store: Store) -> Self {
    MMRBatch {
      memory_batch: Vec::new(),
      store,
    }
  }

  pub fn append(&mut self, pos: u64, elems: Vec<Elem>) {
    self.memory_batch.push((pos, elems));
  }

  pub fn store(&self) -> &Store {
    &self.store
  }
}

impl<Elem: Clone + Send + Sync, Store: MMRStoreReadOps<Elem>>
  MMRBatch<Elem, Store>
{
  pub async fn get_elem(&self, pos: u64) -> Result<Option<Elem>, Store::Error> {
    for (start_pos, elems) in self.memory_batch.iter().rev() {
      if pos < *start_pos {
        continue;
      } else if pos < start_pos + elems.len() as u64 {
        return Ok(elems.get((pos - start_pos) as usize).cloned());
      } else {
        break;
      }
    }
    self.store.get_elem(pos).await
  }
}

impl<Elem: Send, Store: MMRStoreWriteOps<Elem>> MMRBatch<Elem, Store> {
  pub async fn commit(&mut self) -> Result<(), Store::Error> {
    for (pos, elems) in self.memory_batch.drain(..) {
      self.store.append(pos, elems).await?;
    }
    Ok(())
  }
}

impl<Elem, Store> IntoIterator for MMRBatch<Elem, Store> {
  type Item = (u64, Vec<Elem>);
  type IntoIter = crate::vec::IntoIter<Self::Item>;

  fn into_iter(self) -> Self::IntoIter {
    self.memory_batch.into_iter()
  }
}

pub trait MMRStoreReadOps<Elem>: Send + Sync {
  type Error: Error + Send + 'static;
  fn get_elem(
    &self,
    pos: u64,
  ) -> impl Future<Output = Result<Option<Elem>, Self::Error>> + Send;
}

pub trait MMRStoreWriteOps<Elem>: Send + Sync {
  type Error: Error + Send + 'static;
  fn append(
    &mut self,
    pos: u64,
    elems: Vec<Elem>,
  ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
