//! Standard Merkle Mountain Range implementation.
//!
//! [`MMR`] provides an append-only Merkle tree with efficient root computation
//! and inclusion proofs. Elements are added via [`MMR::push`] and the root is
//! computed by "bagging" peaks from right to left.
//!
//! ## Structure
//!
//! An MMR consists of one or more complete binary trees ("mountains").
//! Each mountain's root is a "peak". The MMR root is computed by bagging
//! these peaks from right to left.
//!
//! ## Usage
//!
//! ```rust
//! use merkleberg::{MMR, Merge, DigestMerge, util::{MemStore, MemMMR}};
//! use sha2::Sha256;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create MMR with SHA-256 hashing and in-memory store
//!     let store = MemStore::default();
//!     let mut mmr: MemMMR<DigestMerge<Sha256>> = MMR::new(0, store);
//!
//!     // Add elements (positions are returned for later reference)
//!     let pos0 = mmr.push(b"first").await?;
//!     let pos1 = mmr.push(b"second").await?;
//!
//!     // Persist uncommitted elements to storage
//!     mmr.commit().await?;
//!
//!     // Compute the Merkle root (bagged peaks)
//!     let root = mmr.get_root().await?;
//!
//!     // Generate inclusion proof for the first element
//!     let proof = mmr.gen_proof(vec![pos0]).await?;
//!
//!     // Verify the proof against the root
//!     let leaf_hash = DigestMerge::<Sha256>::leaf_hash(b"first")?;
//!     let is_valid = proof.verify(root, vec![(pos0, leaf_hash)])?;
//!
//!     assert!(is_valid);
//!     Ok(())
//! }
//! ```
//!
//! ## References
//!
//! - [OpenTimestamps MMR spec](https://github.com/opentimestamps/opentimestamps-server/blob/master/doc/merkle-mountain-range.md)
//! - [Grin documentation](https://docs.grin.mw/wiki/chain-state/merkle-mountain-range/)
//! - [Nervos implementation](https://github.com/nervosnetwork/merkle-mountain-range)

use crate::borrow::Cow;
use crate::collections::VecDeque;
use crate::error::UserError;
use crate::helper::{
  get_peak_map, leaf_index_to_mmr_size, leaf_index_to_pos, parent_offset,
  pos_height_in_tree, sibling_offset,
};
use crate::merge::Merge;
use crate::mmr_store::{MMRBatch, MMRStoreReadOps, MMRStoreWriteOps};
use crate::vec;
use crate::vec::Vec;
use crate::{Error, PeaksIter};
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;

/// Merkle Mountain Range data structure.
///
/// An append-only Merkle tree supporting:
/// - Adding new elements via [`Self::push`]
/// - Computing the root hash via [`Self::get_root`]
/// - Generating inclusion proofs via [`Self::gen_proof`]
/// - Verifying proofs via [`InclusionProof::verify`]
///
/// ## Batched Writes
///
/// Elements are buffered until [`Self::commit`] is called, enabling efficient
/// batch writes to storage.
///
/// ## Type Parameters
///
/// - `M`: the [`Merge`] strategy, which defines the item type (`M::Item`) and how two
///   child hashes are combined into a parent hash.
/// - `S`: the backing store, which must implement [`MMRStoreReadOps`] and
///   [`MMRStoreWriteOps`] for `M::Item`.
#[allow(clippy::upper_case_acronyms)]
pub struct MMR<M: Merge, S> {
  mmr_size: u64,
  batch: MMRBatch<M::Item, S>,
  merge: PhantomData<M>,
}

impl<M: Merge, S> MMR<M, S> {
  /// Create a new MMR.
  ///
  /// ## Parameters
  ///
  /// - `mmr_size`: Initial size (0 for empty, existing size if continuing)
  /// - `store`: Storage backend implementing [`MMRStoreReadOps`]
  pub fn new(mmr_size: u64, store: S) -> Self {
    MMR {
      mmr_size,
      batch: MMRBatch::new(store),
      merge: PhantomData,
    }
  }

  /// Returns the current MMR size.
  ///
  /// Size equals the total number of positions (leaves + nodes).
  pub fn mmr_size(&self) -> u64 {
    self.mmr_size
  }

  /// Returns true if the MMR has no elements.
  pub fn is_empty(&self) -> bool {
    self.mmr_size == 0
  }

  /// Access the uncommitted batch.
  ///
  /// Use this to inspect pending elements before [`Self::commit`].
  pub fn batch(&self) -> &MMRBatch<M::Item, S> {
    &self.batch
  }

  /// Access the underlying store.
  pub fn store(&self) -> &S {
    self.batch.store()
  }
}

impl<M: Merge, S: MMRStoreReadOps<M::Item>> MMR<M, S>
where
  M::Item: Clone + PartialEq + Send + Sync,
  M::Error: Into<UserError>,
  S::Error: Into<UserError>,
{
  async fn find_elem<'b>(
    &self,
    pos: u64,
    hashes: &'b [M::Item],
  ) -> Result<Cow<'b, M::Item>, Error> {
    let pos_offset = pos.checked_sub(self.mmr_size);
    if let Some(elem) = pos_offset.and_then(|i| hashes.get(i as usize)) {
      return Ok(Cow::Borrowed(elem));
    }
    let elem = self
      .batch
      .get_elem(pos)
      .await
      .map_err(|e| Error::StoreError(e.into()))?
      .ok_or(Error::InconsistentStore)?;
    Ok(Cow::Owned(elem))
  }

  /// Add a new element to the MMR.
  ///
  /// ## Returns
  ///
  /// The position of the new leaf element.
  ///
  /// ## Errors
  ///
  /// - [`Error::MergeError`] if leaf hashing fails
  /// - [`Error::StoreError`] if element fetch fails during merging
  pub async fn push(&mut self, data: &[u8]) -> Result<u64, Error> {
    let elem = M::leaf_hash(data).map_err(|e| Error::MergeError(e.into()))?;
    let mut elems = vec![elem];
    let elem_pos = self.mmr_size;
    let peak_map = get_peak_map(self.mmr_size);
    let mut pos = self.mmr_size;
    let mut peak = 1;
    while (peak_map & peak) != 0 {
      peak <<= 1u64;
      pos += 1;
      let left_pos = pos - peak;
      let left_elem = self.find_elem(left_pos, &elems).await?;
      let right_elem = elems.last().expect("checked");
      let parent_elem = M::merge(&left_elem, right_elem)
        .map_err(|e| Error::MergeError(e.into()))?;
      elems.push(parent_elem);
    }
    self.batch.append(elem_pos, elems);
    self.mmr_size = pos + 1;
    Ok(elem_pos)
  }

  /// Compute the Merkle root.
  ///
  /// The root is computed by bagging all peaks from right to left.
  ///
  /// ## Errors
  ///
  /// - [`Error::GetRootOnEmpty`] if MMR has no elements
  /// - [`Error::InconsistentStore`] if peaks are missing from store
  /// - [`Error::StoreError`] if store fetch fails
  pub async fn get_root(&self) -> Result<M::Item, Error> {
    if self.mmr_size == 0 {
      return Err(Error::GetRootOnEmpty);
    } else if self.mmr_size == 1 {
      return self
        .batch
        .get_elem(0)
        .await
        .map_err(|e| Error::StoreError(e.into()))?
        .ok_or(Error::InconsistentStore);
    }
    let elems = self
      .batch
      .get_elems(PeaksIter::new(self.mmr_size).collect())
      .await
      .map_err(|e| Error::StoreError(e.into()))?;
    let peaks: Vec<M::Item> = elems
      .into_iter()
      .map(|elem| elem.ok_or(Error::InconsistentStore))
      .collect::<Result<Vec<_>, _>>()?;
    self
      .bag_rhs_peaks(peaks)
      .map_err(|e| Error::MergeError(e.into()))?
      .ok_or(Error::InconsistentStore)
  }

  fn bag_rhs_peaks(
    &self,
    mut rhs_peaks: Vec<M::Item>,
  ) -> Result<Option<M::Item>, M::Error> {
    while rhs_peaks.len() > 1 {
      let right_peak = rhs_peaks.pop().expect("pop");
      let left_peak = rhs_peaks.pop().expect("pop");
      rhs_peaks.push(M::merge_peaks(&right_peak, &left_peak)?);
    }
    Ok(rhs_peaks.pop())
  }

  async fn gen_proof_for_peak(
    &self,
    proof: &mut Vec<M::Item>,
    pos_list: Vec<u64>,
    peak_pos: u64,
  ) -> Result<(), Error> {
    if pos_list.len() == 1 && pos_list == [peak_pos] {
      return Ok(());
    }
    if pos_list.is_empty() {
      proof.push(
        self
          .batch
          .get_elem(peak_pos)
          .await
          .map_err(|e| Error::StoreError(e.into()))?
          .ok_or(Error::InconsistentStore)?,
      );
      return Ok(());
    }

    let mut queue: VecDeque<_> =
      pos_list.into_iter().map(|pos| (pos, 0)).collect();

    while let Some((pos, height)) = queue.pop_front() {
      debug_assert!(pos <= peak_pos);
      if pos == peak_pos {
        if queue.is_empty() {
          break;
        }
        return Err(Error::NodeProofsNotSupported);
      }

      let (sib_pos, parent_pos) = {
        let next_height = pos_height_in_tree(pos + 1);
        let sibling_offset = sibling_offset(height);
        if next_height > height {
          (pos - sibling_offset, pos + 1)
        } else {
          (pos + sibling_offset, pos + parent_offset(height))
        }
      };

      if Some(&sib_pos) == queue.front().map(|(pos, _)| pos) {
        queue.pop_front();
      } else {
        proof.push(
          self
            .batch
            .get_elem(sib_pos)
            .await
            .map_err(|e| Error::StoreError(e.into()))?
            .ok_or(Error::InconsistentStore)?,
        );
      }
      if parent_pos < peak_pos {
        queue.push_back((parent_pos, height + 1));
      }
    }
    Ok(())
  }

  /// Generate an inclusion proof for leaves.
  ///
  /// ## Parameters
  ///
  /// - `pos_list`: Positions of leaves to prove
  ///
  /// ## Returns
  ///
  /// An [`InclusionProof`] containing the Merkle path.
  ///
  /// ## Errors
  ///
  /// - [`Error::GenProofForInvalidLeaves`] if positions are invalid
  /// - [`Error::NodeProofsNotSupported`] if proving non-leaf nodes
  /// - [`Error::StoreError`] if required nodes are missing
  pub async fn gen_proof(
    &mut self,
    mut pos_list: Vec<u64>,
  ) -> Result<InclusionProof<M>, Error> {
    if pos_list.is_empty() {
      return Err(Error::GenProofForInvalidLeaves);
    }
    if self.mmr_size == 1 && pos_list == [0] {
      return Ok(InclusionProof::new(self.mmr_size, Vec::new()));
    }
    if pos_list.iter().any(|pos| pos_height_in_tree(*pos) > 0) {
      return Err(Error::NodeProofsNotSupported);
    }
    pos_list.sort_unstable();
    pos_list.dedup();
    let peaks: Vec<u64> = PeaksIter::new(self.mmr_size).collect();
    let mut proof: Vec<M::Item> = Vec::new();
    let mut bagging_track = 0;
    for peak_pos in peaks {
      let pos_list: Vec<_> =
        take_while_vec(&mut pos_list, |&pos| pos <= peak_pos);
      if pos_list.is_empty() {
        bagging_track += 1;
      } else {
        bagging_track = 0;
      }
      self
        .gen_proof_for_peak(&mut proof, pos_list, peak_pos)
        .await?;
    }

    if !pos_list.is_empty() {
      return Err(Error::GenProofForInvalidLeaves);
    }

    if bagging_track > 1 {
      let rhs_peaks = proof.split_off(proof.len() - bagging_track);
      proof.push(
        self
          .bag_rhs_peaks(rhs_peaks)
          .map_err(|e| Error::MergeError(e.into()))?
          .expect("bagging rhs peaks"),
      );
    }

    Ok(InclusionProof::new(self.mmr_size, proof))
  }
}

impl<M: Merge, S: MMRStoreWriteOps<M::Item>> MMR<M, S>
where
  M::Item: Send,
  S::Error: Into<UserError>,
{
  /// Persist uncommitted elements to storage.
  ///
  /// Elements added via [`Self::push`] are buffered until commit.
  /// Call this to write them to the underlying store.
  ///
  /// ## Errors
  ///
  /// - [`Error::StoreError`] if write fails
  pub async fn commit(&mut self) -> Result<(), Error> {
    self
      .batch
      .commit()
      .await
      .map_err(|e| Error::StoreError(e.into()))
  }
}

/// Proof that elements exist in the MMR.
///
/// Contains the Merkle path from leaves to peaks,
/// plus peak bagging hashes.
///
/// ## Verification
///
/// Use [`Self::verify`] to check against a known root:
///
/// ```rust,ignore
/// let proof = mmr.gen_proof(vec![pos]).await?;
/// let root = mmr.get_root().await?;
///
/// let leaves = vec![(pos, element)];
/// assert!(proof.verify(root, leaves));
/// ```
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(bound(serialize = "M::Item: serde::Serialize"))
)]
#[cfg_attr(
  feature = "serde",
  serde(bound(deserialize = "M::Item: serde::Deserialize<'de>"))
)]
pub struct InclusionProof<M: Merge> {
  mmr_size: u64,
  proof: Vec<M::Item>,
  #[cfg_attr(feature = "serde", serde(skip))]
  merge: PhantomData<M>,
}

impl<M: Merge> InclusionProof<M>
where
  M::Item: Clone + PartialEq,
  M::Error: Into<UserError>,
{
  /// Create a proof from raw components.
  ///
  /// Usually obtained via [`MMR::gen_proof`].
  #[must_use]
  pub fn new(mmr_size: u64, proof: Vec<M::Item>) -> Self {
    InclusionProof {
      mmr_size,
      proof,
      merge: PhantomData,
    }
  }

  /// MMR size when proof was generated.
  #[must_use]
  pub fn mmr_size(&self) -> u64 {
    self.mmr_size
  }

  /// Proof items (Merkle path and peak hashes).
  #[must_use]
  pub fn proof_items(&self) -> &[M::Item] {
    &self.proof
  }

  /// Compute root from proof and leaves.
  ///
  /// ## Parameters
  ///
  /// - `leaves`: Vector of `(position, element)` pairs
  ///
  /// ## Returns
  ///
  /// The computed root hash.
  ///
  /// ## Errors
  ///
  /// - [`Error::CorruptedProof`] if proof structure is invalid
  /// - [`Error::MergeError`] if merging fails
  pub fn calculate_root(
    &self,
    leaves: Vec<(u64, M::Item)>,
  ) -> Result<M::Item, Error> {
    calculate_root::<M, _>(leaves, self.mmr_size, self.proof.iter())
  }

  /// Compute root with a new leaf not in original MMR.
  ///
  /// Used for incremental verification.
  pub fn calculate_root_with_new_leaf(
    &self,
    mut leaves: Vec<(u64, M::Item)>,
    new_pos: u64,
    new_elem: M::Item,
    new_mmr_size: u64,
  ) -> Result<M::Item, Error> {
    let pos_height = pos_height_in_tree(new_pos);
    let next_height = pos_height_in_tree(new_pos + 1);
    if next_height > pos_height {
      let mut peaks_hashes = calculate_peaks_hashes::<M, _>(
        leaves,
        self.mmr_size,
        self.proof.iter(),
      )?;
      let peaks_pos: Vec<u64> = PeaksIter::new(new_mmr_size).collect();
      let mut i = 0;
      while peaks_pos[i] < new_pos {
        i += 1;
      }
      peaks_hashes[i..].reverse();
      calculate_root::<M, _>(
        vec![(new_pos, new_elem)],
        new_mmr_size,
        peaks_hashes.iter(),
      )
    } else {
      leaves.push((new_pos, new_elem));
      calculate_root::<M, _>(leaves, new_mmr_size, self.proof.iter())
    }
  }

  /// Verify that leaves produce the given root.
  ///
  /// ## Returns
  ///
  /// `true` if computed root matches, `false` otherwise.
  pub fn verify(
    &self,
    root: &M::Item,
    leaves: Vec<(u64, M::Item)>,
  ) -> Result<bool, Error> {
    self
      .calculate_root(leaves)
      .map(|calculated_root| calculated_root == *root)
  }

  /// Verify incrementally with new elements.
  ///
  /// Used to verify a proof from older MMR state against newer root.
  pub fn verify_incremental(
    &self,
    root: &M::Item,
    prev_root: &M::Item,
    incremental: Vec<M::Item>,
  ) -> Result<bool, Error> {
    let current_leaves_count = get_peak_map(self.mmr_size);
    if current_leaves_count <= incremental.len() as u64 {
      return Err(Error::CorruptedProof);
    }
    let prev_leaves_count = current_leaves_count - incremental.len() as u64;
    let prev_peaks_positions = {
      let prev_index = prev_leaves_count - 1;
      let prev_mmr_size = leaf_index_to_mmr_size(prev_index);
      let prev_peaks_positions: Vec<u64> =
        PeaksIter::new(prev_mmr_size).collect();
      if prev_peaks_positions.len() != self.proof.len() {
        return Err(Error::CorruptedProof);
      }
      prev_peaks_positions
    };
    let current_peaks_positions: Vec<u64> =
      PeaksIter::new(self.mmr_size).collect();

    let mut reverse_index = prev_peaks_positions.len() - 1;
    for (i, position) in prev_peaks_positions.iter().enumerate() {
      if *position < current_peaks_positions[i] {
        reverse_index = i;
        break;
      }
    }
    let mut prev_peaks: Vec<_> = self.proof_items().to_vec();
    let mut reverse_peaks = prev_peaks.split_off(reverse_index);
    reverse_peaks.reverse();
    prev_peaks.extend(reverse_peaks);

    let calculated_prev_root = bagging_peaks_hashes::<M>(prev_peaks)
      .map_err(|e| Error::MergeError(e.into()))?;
    if calculated_prev_root != *prev_root {
      return Ok(false);
    }

    let leaves = incremental
      .into_iter()
      .enumerate()
      .map(|(index, leaf)| {
        let pos = leaf_index_to_pos(prev_leaves_count + index as u64);
        (pos, leaf)
      })
      .collect();
    self
      .verify(root, leaves)
      .map_err(|e| Error::MergeError(e.into()))
  }
}

fn calculate_peak_root<'a, M: Merge, I: Iterator<Item = &'a M::Item>>(
  leaves: Vec<(u64, M::Item)>,
  peak_pos: u64,
  proof_iter: &mut I,
) -> Result<M::Item, Error>
where
  M::Item: 'a + Clone,
  M::Error: Into<UserError>,
{
  debug_assert!(!leaves.is_empty(), "can't be empty");

  let mut queue: VecDeque<_> = leaves
    .into_iter()
    .map(|(pos, item)| (pos, item, 0))
    .collect();

  while let Some((pos, item, height)) = queue.pop_front() {
    if pos == peak_pos {
      if queue.is_empty() {
        return Ok(item);
      }
      return Err(Error::CorruptedProof);
    }
    let next_height = pos_height_in_tree(pos + 1);
    let (parent_pos, parent_item) = {
      let sibling_offset = sibling_offset(height);
      if next_height > height {
        let sib_pos = pos - sibling_offset;
        let parent_pos = pos + 1;
        let parent_item = if Some(&sib_pos)
          == queue.front().map(|(pos, _, _)| pos)
        {
          let sibling_item =
            queue.pop_front().map(|(_, item, _)| item).unwrap();
          M::merge(&sibling_item, &item)
            .map_err(|e| Error::MergeError(e.into()))?
        } else {
          let sibling_item = proof_iter.next().ok_or(Error::CorruptedProof)?;
          M::merge(sibling_item, &item)
            .map_err(|e| Error::MergeError(e.into()))?
        };
        (parent_pos, parent_item)
      } else {
        let sib_pos = pos + sibling_offset;
        let parent_pos = pos + parent_offset(height);
        let parent_item = if Some(&sib_pos)
          == queue.front().map(|(pos, _, _)| pos)
        {
          let sibling_item =
            queue.pop_front().map(|(_, item, _)| item).unwrap();
          M::merge(&item, &sibling_item)
            .map_err(|e| Error::MergeError(e.into()))?
        } else {
          let sibling_item = proof_iter.next().ok_or(Error::CorruptedProof)?;
          M::merge(&item, sibling_item)
            .map_err(|e| Error::MergeError(e.into()))?
        };
        (parent_pos, parent_item)
      }
    };

    if parent_pos <= peak_pos {
      queue.push_back((parent_pos, parent_item, height + 1));
    } else {
      return Err(Error::CorruptedProof);
    }
  }
  Err(Error::CorruptedProof)
}

fn calculate_peaks_hashes<'a, M: Merge, I: Iterator<Item = &'a M::Item>>(
  mut leaves: Vec<(u64, M::Item)>,
  mmr_size: u64,
  mut proof_iter: I,
) -> Result<Vec<M::Item>, Error>
where
  M::Item: 'a + Clone,
  M::Error: Into<UserError>,
{
  if leaves.iter().any(|(pos, _)| pos_height_in_tree(*pos) > 0) {
    return Err(Error::NodeProofsNotSupported);
  }

  if mmr_size == 1 && leaves.len() == 1 && leaves[0].0 == 0 {
    return Ok(leaves.into_iter().map(|(_pos, item)| item).collect());
  }
  leaves.sort_by_key(|(pos, _)| *pos);
  leaves.dedup_by(|a, b| a.0 == b.0);
  let peaks = PeaksIter::new(mmr_size);

  let mut peaks_hashes: Vec<M::Item> = Vec::with_capacity(peaks.len() + 1);
  for peak_pos in peaks {
    let mut leaves: Vec<_> =
      take_while_vec(&mut leaves, |(pos, _)| *pos <= peak_pos);
    let peak_root = if leaves.len() == 1 && leaves[0].0 == peak_pos {
      leaves.remove(0).1
    } else if leaves.is_empty() {
      if let Some(peak_root) = proof_iter.next() {
        peak_root.clone()
      } else {
        break;
      }
    } else {
      calculate_peak_root::<M, _>(leaves, peak_pos, &mut proof_iter)?
    };
    peaks_hashes.push(peak_root.clone());
  }

  if !leaves.is_empty() {
    return Err(Error::CorruptedProof);
  }

  if let Some(rhs_peaks_hashes) = proof_iter.next() {
    peaks_hashes.push(rhs_peaks_hashes.clone());
  }
  if proof_iter.next().is_some() {
    return Err(Error::CorruptedProof);
  }
  Ok(peaks_hashes)
}

fn bagging_peaks_hashes<M: Merge>(
  mut peaks_hashes: Vec<M::Item>,
) -> Result<M::Item, Error>
where
  M::Error: Into<UserError>,
{
  while peaks_hashes.len() > 1 {
    let right_peak = peaks_hashes.pop().expect("pop");
    let left_peak = peaks_hashes.pop().expect("pop");
    peaks_hashes.push(
      M::merge_peaks(&right_peak, &left_peak)
        .map_err(|e| Error::MergeError(e.into()))?,
    );
  }
  peaks_hashes.pop().ok_or(Error::CorruptedProof)
}

fn calculate_root<'a, M: Merge, I: Iterator<Item = &'a M::Item>>(
  leaves: Vec<(u64, M::Item)>,
  mmr_size: u64,
  proof_iter: I,
) -> Result<M::Item, Error>
where
  M::Item: 'a + Clone,
  M::Error: Into<UserError>,
{
  let peaks_hashes =
    calculate_peaks_hashes::<M, _>(leaves, mmr_size, proof_iter)?;
  bagging_peaks_hashes::<M>(peaks_hashes)
}

fn take_while_vec<T, P: Fn(&T) -> bool>(v: &mut Vec<T>, p: P) -> Vec<T> {
  for i in 0..v.len() {
    if !p(&v[i]) {
      return v.drain(..i).collect();
    }
  }
  mem::take(v)
}
