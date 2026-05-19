use crate::vec::Vec;
use core::error::Error;
use core::future::Future;

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

  pub async fn get_elems(
    &self,
    positions: Vec<u64>,
  ) -> Result<Vec<Option<Elem>>, Store::Error> {
    let mut results: Vec<Option<Elem>> = vec![None; positions.len()];
    let mut missing_indices: Vec<usize> = Vec::new();
    let mut missing_positions: Vec<u64> = Vec::new();

    for (i, pos) in positions.into_iter().enumerate() {
      let found = self.memory_batch.iter().rev().any(|(start_pos, elems)| {
        if pos >= *start_pos && pos < start_pos + elems.len() as u64 {
          results[i] = Some(elems[(pos - start_pos) as usize].clone());
          true
        } else {
          false
        }
      });
      if !found {
        missing_indices.push(i);
        missing_positions.push(pos);
      }
    }

    if !missing_positions.is_empty() {
      let fetched = self.store.get_elems(missing_positions.into_iter()).await?;
      for (idx, elem) in missing_indices.iter().zip(fetched.iter()) {
        results[*idx] = elem.clone();
      }
    }

    Ok(results)
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

pub trait MMRStoreReadOps<Elem: Send>: Send + Sync {
  type Error: Error + Send + Sync + 'static;
  fn get_elem(
    &self,
    pos: u64,
  ) -> impl Future<Output = Result<Option<Elem>, Self::Error>> + Send;

  fn get_elems(
    &self,
    positions: impl Iterator<Item = u64> + Send,
  ) -> impl Future<Output = Result<Vec<Option<Elem>>, Self::Error>> + Send {
    async move {
      let futures = positions.map(|pos| self.get_elem(pos));
      futures_util::future::join_all(futures)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
    }
  }
}

pub trait MMRStoreWriteOps<Elem>: Send + Sync {
  type Error: Error + Send + Sync + 'static;
  fn append(
    &mut self,
    pos: u64,
    elems: Vec<Elem>,
  ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
