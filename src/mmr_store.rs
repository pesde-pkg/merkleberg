use crate::vec::Vec;
use core::error::Error;
use core::future::Future;

/// Batch of uncommitted MMR elements.
///
/// Elements added via [`push`] are buffered into memory until [`commit`]
/// is called.
///
/// Useful since individual writes need to hold an exclusive lock for a
/// commit. Batching allows for holding a singular lock for multiple commits.
///
/// ## Inspecting Batch
///
/// ```rust,ignore
/// let batch = mmr.batch();
/// for (pos, elems) in batch {
///     println!("Uncommitted at {}: {} elements", pos, elems.len());
/// }
/// ```
pub struct MMRBatch<Elem, Store> {
  memory_batch: Vec<(u64, Vec<Elem>)>,
  store: Store,
}

impl<Elem, Store> MMRBatch<Elem, Store> {
  /// Create batch with store backend.
  pub fn new(store: Store) -> Self {
    MMRBatch {
      memory_batch: Vec::new(),
      store,
    }
  }

  /// Append elements to batch (not yet committed).
  pub fn append(&mut self, pos: u64, elems: Vec<Elem>) {
    self.memory_batch.push((pos, elems));
  }

  /// Access underlying store.
  pub fn store(&self) -> &Store {
    &self.store
  }

  /// Consume the batch, returning the backed store.
  ///
  /// Make sure to commit all the changes to the store first before
  /// calling this method to ensure changes in the batch queue are not
  /// discarded unexpectedly.
  pub fn into_store(self) -> Store {
    self.store
  }
}

impl<Elem: Clone + Send + Sync, Store: MMRStoreReadOps<Elem>>
  MMRBatch<Elem, Store>
{
  /// Fetch element from batch or store.
  ///
  /// Checks batch first, then falls back to store.
  pub async fn get_elem(&self, pos: u64) -> Result<Option<Elem>, Store::Error> {
    for (start_pos, elems) in self.memory_batch.iter().rev() {
      if pos < *start_pos {
        continue;
      }
      if pos < start_pos + elems.len() as u64 {
        return Ok(elems.get((pos - start_pos) as usize).cloned());
      }
      break;
    }
    self.store.get_elem(pos).await
  }

  /// Fetch multiple elements from batch or store.
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
        results[*idx].clone_from(elem);
      }
    }

    Ok(results)
  }
}

impl<Elem: Send, Store: MMRStoreWriteOps<Elem>> MMRBatch<Elem, Store> {
  /// Commit batch to storage.
  ///
  /// Writes all buffered elements to store via [`MMRStoreWriteOps::append`].
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

/// Trait for reading elements from MMR storage.
///
/// Implement this to provide a custom backend (database, file, etc.).
///
/// ## Methods
///
/// - [`Self::get_elem`]: Fetch single element by position
/// - [`Self::get_elems`]: Fetch multiple elements (default: sequential fetches)
///
/// Override [`Self::get_elems`] for batch-optimized backends (e.g., SQL).
///
/// ## References
///
/// See [`crate::util::MemStore`] for a simple in-memory implementation.
pub trait MMRStoreReadOps<Elem: Send>: Send + Sync {
  type Error: Error + Send + Sync + 'static;

  /// Fetch a single element by position.
  ///
  /// Returns `None` if position doesn't exist.
  fn get_elem(
    &self,
    pos: u64,
  ) -> impl Future<Output = Result<Option<Elem>, Self::Error>> + Send;

  /// Fetch multiple elements by positions.
  ///
  /// Default implementation calls [`Self::get_elem`] sequentially.
  /// Override for batch-optimized storage.
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

/// Trait for writing elements to MMR storage.
///
/// Implement this to enable [`crate::MMR::commit`] for your backend.
pub trait MMRStoreWriteOps<Elem>: Send + Sync {
  type Error: Error + Send + Sync + 'static;

  /// Append elements to storage.
  ///
  /// Called by [`crate::MMR::commit`] to persist buffered elements.
  fn append(
    &mut self,
    pos: u64,
    elems: Vec<Elem>,
  ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
