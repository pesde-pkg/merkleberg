//! MMRIVER - Merkle Mountain Range for Immediately Verifiable and Replicable Commitments.
//!
//! [`MMRIVER`] provides an alternative MMR structure with:
//! - Accumulator-based roots (list of peaks, not single hash)
//! - Consistency proofs between tree states
//!
//! Unlike standard [`MMR`] which bags peaks into a single root, MMRIVER
//! keeps peaks as a list (accumulator). This enables:
//! - Verification of individual peak membership
//! - Consistency proofs showing tree evolution
//!
//! Consistency proofs show that old headers are included in new state.
//!
//! ## Usage
//!
//! ```rust
//! use merkleberg::{MMRIVER, Merge, DigestMerge, util::MemStore};
//! use sha2::Sha256;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create MMRIVER with SHA-256 hashing
//!     let store = MemStore::default();
//!     let mut mmriver: MMRIVER<DigestMerge<Sha256>, _> = MMRIVER::new(0, store);
//!
//!     // Add elements
//!     for i in 0u64..10 {
//!         mmriver.push(&i.to_be_bytes()).await?;
//!     }
//!     mmriver.commit().await?;
//!
//!     // Get the accumulator (list of peaks) - unique to MMRIVER
//!     let accumulator = mmriver.get_accumulator().await?;
//!     
//!     // Generate inclusion proof for leaf at node index 0
//!     let proof = mmriver.gen_inclusion_proof(0).await?;
//!     
//!     // Compute leaf hash for verification
//!     let leaf_hash = DigestMerge::<Sha256>::leaf_hash(&0u64.to_be_bytes())?;
//!     
//!     // Verify against the accumulator
//!     let is_valid = proof.verify(leaf_hash, &accumulator)?;
//!     assert!(is_valid);
//!
//!     // Save state for consistency proof demonstration
//!     let old_size = mmriver.mmr_size();
//!     let old_accumulator = accumulator;
//!
//!     // Add more elements
//!     for i in 10u64..20 {
//!         mmriver.push(&i.to_be_bytes()).await?;
//!     }
//!     mmriver.commit().await?;
//!
//!     // Generate consistency proof showing old state in new state
//!     let new_accumulator = mmriver.get_accumulator().await?;
//!     let consistency_proof = mmriver.gen_consistency_proof(old_size).await?;
//!
//!     // Verify old accumulator is consistent with new accumulator
//!     let is_consistent = consistency_proof.verify(old_accumulator, &new_accumulator)?;
//!     assert!(is_consistent);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## References
//!
//! - [IETF MMRIVER draft](https://github.com/robinbryce/merkle-mountain-range-proofs)
//! - [Wikipedia: Merkle tree](https://en.wikipedia.org/wiki/Merkle_tree)

use crate::Error;
use crate::borrow::Cow;
use crate::error::UserError;
use crate::helper::{
  PeaksMMRIVERIter, inclusion_proof_path, index_height_mmriver,
};
use crate::merge::Merge;
use crate::mmr_store::{MMRBatch, MMRStoreReadOps, MMRStoreWriteOps};
use crate::vec;
use crate::vec::Vec;
use core::marker::PhantomData;

/// An MMR structure that retains peaks as an accumulator rather than bagging them into a single root.
///
/// ## Accumulator vs. Bagged Root
///
/// A standard `MMR` collapses all peaks into a single hash (bagging). `MMRIVER` instead
/// retains peaks as an ordered list (i.e., the accumulator) where each entry is the root
/// hash of one perfect binary subtree. This means the root of an `MMRIVER` is a list rather
/// than a singular value.
///
/// ## Capabilities
///
/// Retaining peaks individually enables two things a bagged root cannot support efficiently:
///
/// 1. Peak membership: prove that a specific peak hash belongs to the current accumulator
///    without rehashing unrelated peaks.
/// 2. Consistency proofs: given an old accumulator and a new one, produce a proof that
///    every element committed in the old state is also committed in the new state, without
///    having to process the the full append history.
///
/// ## Type Parameters
///
/// - `M`: the [`Merge`] strategy, which defines the item type (`M::Item`) and how two
///   child hashes are combined into a parent hash.
/// - `S`: the backing store, which must implement [`MMRStoreReadOps`] and
///   [`MMRStoreWriteOps`] for `M::Item`.
pub struct MMRIVER<M: Merge, S> {
  mmr_size: u64,
  batch: MMRBatch<M::Item, S>,
  _merge: PhantomData<M>,
}

impl<M: Merge, S> MMRIVER<M, S> {
  /// Create a new MMRIVER.
  ///
  /// ## Parameters
  ///
  /// - `mmr_size`: Initial size (0 for empty, existing size if continuing)
  /// - `store`: Storage backend implementing [`MMRStoreReadOps`]
  pub fn new(mmr_size: u64, store: S) -> Self {
    MMRIVER {
      mmr_size,
      batch: MMRBatch::new(store),
      _merge: PhantomData,
    }
  }

  /// Returns the current MMR size.
  pub fn mmr_size(&self) -> u64 {
    self.mmr_size
  }

  /// Returns true if the MMR has no elements.
  pub fn is_empty(&self) -> bool {
    self.mmr_size == 0
  }

  /// Access the uncommitted batch.
  pub fn batch(&self) -> &MMRBatch<M::Item, S> {
    &self.batch
  }

  /// Access the underlying store.
  pub fn store(&self) -> &S {
    self.batch.store()
  }
}

impl<M: Merge, S: MMRStoreReadOps<M::Item>> MMRIVER<M, S>
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

    self
      .batch
      .get_elem(pos)
      .await
      .map_err(|e| Error::StoreError(e.into()))?
      .ok_or(Error::InconsistentStore)
      .map(Cow::Owned)
  }

  /// Add a new element.
  ///
  /// Returns the position of the new leaf.
  pub async fn push(&mut self, data: &[u8]) -> Result<u64, Error> {
    let elem = M::leaf_hash(data).map_err(|e| Error::MergeError(e.into()))?;
    let elem_pos = self.mmr_size;
    let mut elems = vec![elem];
    let mut i = self.mmr_size + 1;
    let mut g = 0u8;

    while index_height_mmriver(i) > g {
      let left_pos = i - (2 << g);
      let right_pos = i - 1;
      let (left_elem, right_elem) = futures_util::future::join(
        self.find_elem(left_pos, &elems),
        self.find_elem(right_pos, &elems),
      )
      .await;
      let left_elem = left_elem?;
      let right_elem = right_elem?;
      let parent_elem = M::merge_pos(i + 1, &left_elem, &right_elem)
        .map_err(|e| Error::MergeError(e.into()))?;
      elems.push(parent_elem);
      i += 1;
      g += 1;
    }

    self.batch.append(elem_pos, elems);
    self.mmr_size = i;
    Ok(elem_pos)
  }

  /// Get the accumulator (list of peaks).
  ///
  /// Unlike [`crate::MMR::get_root`], this returns all peaks as a vector.
  /// The root can be computed separately by bagging these peaks.
  pub async fn get_accumulator(&self) -> Result<Vec<M::Item>, Error> {
    if self.mmr_size == 0 {
      return Err(Error::GetRootOnEmpty);
    }
    let elems = self
      .batch
      .get_elems(PeaksMMRIVERIter::new(self.mmr_size - 1).collect())
      .await
      .map_err(|e| Error::StoreError(e.into()))?;
    let peaks: Vec<M::Item> = elems
      .into_iter()
      .map(|elem| elem.ok_or(Error::InconsistentStore))
      .collect::<Result<Vec<_>, _>>()?;
    Ok(peaks)
  }

  /// Get the root (bagged accumulator).
  ///
  /// Bags peaks from right to left to produce single hash.
  pub async fn get_root(&self) -> Result<M::Item, Error> {
    let peaks = self.get_accumulator().await?;
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

  /// Generate consistency proof between two states.
  ///
  /// Proves that `mmr_size_from` state is included in current state.
  ///
  /// ## Parameters
  ///
  /// - `mmr_size_from`: Previous MMR size
  ///
  /// ## Returns
  ///
  /// [`ConsistencyProof`] showing state evolution.
  pub async fn gen_consistency_proof(
    &self,
    mmr_size_from: u64,
  ) -> Result<ConsistencyProof<M>, Error> {
    if mmr_size_from == 0 || mmr_size_from > self.mmr_size {
      return Err(Error::GenProofForInvalidLeaves);
    }

    let ifrom = mmr_size_from - 1;
    let ito = self.mmr_size - 1;

    let proof_indices = PeaksMMRIVERIter::new(ifrom)
      .map(|ipeak| inclusion_proof_path(ipeak, ito));

    let all_elems = self
      .batch
      .get_elems(proof_indices.clone().flatten().collect())
      .await
      .map_err(|e| Error::StoreError(e.into()))?;

    let mut proof_paths: Vec<Vec<M::Item>> =
      Vec::with_capacity(proof_indices.len());
    let mut offset = 0;
    for path_indices in proof_indices {
      let path_values: Vec<M::Item> = all_elems
        [offset..offset + path_indices.len()]
        .iter()
        .cloned()
        .map(|elem| elem.ok_or(Error::InconsistentStore))
        .collect::<Result<Vec<_>, _>>()?;
      proof_paths.push(path_values);
      offset += path_indices.len();
    }

    Ok(ConsistencyProof::new(
      mmr_size_from,
      self.mmr_size,
      proof_paths,
    ))
  }

  /// Generate inclusion proof for a leaf.
  ///
  /// ## Parameters
  ///
  /// - `i`: Index of the leaf to prove
  ///
  /// ## Returns
  ///
  /// [`InclusionProof`] with Merkle path to peak.
  pub async fn gen_inclusion_proof(
    &self,
    i: u64,
  ) -> Result<InclusionProof<M>, Error> {
    if i >= self.mmr_size {
      return Err(Error::GenProofForInvalidLeaves);
    }

    let c = self.mmr_size - 1;
    let path_indices = crate::helper::inclusion_proof_path(i, c);

    let elems = self
      .batch
      .get_elems(path_indices)
      .await
      .map_err(|e| Error::StoreError(e.into()))?;
    let path_values: Vec<M::Item> = elems
      .into_iter()
      .map(|elem| elem.ok_or(Error::InconsistentStore))
      .collect::<Result<Vec<_>, _>>()?;

    Ok(InclusionProof::new(i, path_values))
  }
}

impl<M: Merge, S: MMRStoreWriteOps<M::Item>> MMRIVER<M, S>
where
  M::Item: Send,
  S::Error: Into<UserError>,
{
  /// Persist uncommitted elements.
  pub async fn commit(&mut self) -> Result<(), Error> {
    self
      .batch
      .commit()
      .await
      .map_err(|e| Error::StoreError(e.into()))
  }
}

/// Inclusion proof for MMRIVER.
///
/// Proves that a leaf exists in the accumulator.
///
/// Unlike [`crate::mmr::InclusionProof`], which checks against a singular root,
/// this checks against an accumulator (list of peaks) instead.
#[derive(Debug)]
pub struct InclusionProof<M: Merge> {
  index: u64,
  proof: Vec<M::Item>,
  _merge: PhantomData<M>,
}

impl<M: Merge> InclusionProof<M>
where
  M::Item: Clone + PartialEq,
  M::Error: Into<UserError>,
{
  /// Create proof from raw components.
  pub fn new(index: u64, proof: Vec<M::Item>) -> Self {
    InclusionProof {
      index,
      proof,
      _merge: PhantomData,
    }
  }

  /// Leaf index being proven.
  pub fn index(&self) -> u64 {
    self.index
  }

  /// Merkle path elements.
  pub fn proof(&self) -> &[M::Item] {
    &self.proof
  }

  /// Compute the root hash from leaf and proof.
  pub fn included_root(&self, nodehash: M::Item) -> Result<M::Item, M::Error> {
    included_root::<M>(self.index, nodehash, &self.proof)
  }

  /// Verify against an accumulator.
  ///
  /// ## Parameters
  ///
  /// - `nodehash`: The leaf hash being proven (use `M::leaf_hash(data)` to compute)
  /// - `accumulator`: The accumulator (list of peaks) to verify against
  ///
  /// ## Returns
  ///
  /// `true` if computed peak matches any peak in accumulator.
  pub fn verify(
    &self,
    nodehash: M::Item,
    accumulator: &[M::Item],
  ) -> Result<bool, Error> {
    let root = self
      .included_root(nodehash)
      .map_err(|e| Error::MergeError(e.into()))?;

    let peak_positions = PeaksMMRIVERIter::new(self.index);
    if peak_positions.len() == 0 {
      return Ok(false);
    }

    for peak in accumulator {
      if *peak == root {
        return Ok(true);
      }
    }
    Ok(false)
  }
}

/// Proof that two MMRIVER states are consistent.
///
/// Shows that an older accumulator is a prefix of a newer accumulator.
/// Used to verify blockchain header chain continuity.
#[derive(Debug)]
pub struct ConsistencyProof<M: Merge> {
  mmr_size_from: u64,
  mmr_size_to: u64,
  proof_paths: Vec<Vec<M::Item>>,
  _merge: PhantomData<M>,
}

impl<M: Merge> ConsistencyProof<M>
where
  M::Item: Clone + PartialEq,
  M::Error: Into<UserError>,
{
  /// Create proof from raw components.
  pub fn new(
    mmr_size_from: u64,
    mmr_size_to: u64,
    proof_paths: Vec<Vec<M::Item>>,
  ) -> Self {
    ConsistencyProof {
      mmr_size_from,
      mmr_size_to,
      proof_paths,
      _merge: PhantomData,
    }
  }

  /// MMR size at proof generation start.
  pub fn mmr_size_from(&self) -> u64 {
    self.mmr_size_from
  }

  /// MMR size at proof generation end.
  pub fn mmr_size_to(&self) -> u64 {
    self.mmr_size_to
  }

  /// Proof paths for each old peak.
  pub fn proof_paths(&self) -> &[Vec<M::Item>] {
    &self.proof_paths
  }

  /// Compute roots that should match old accumulator.
  pub fn consistent_roots(
    &self,
    old_accumulator: Vec<M::Item>,
  ) -> Result<Vec<M::Item>, Error> {
    let from_peaks: Vec<u64> =
      PeaksMMRIVERIter::new(self.mmr_size_from - 1).collect();
    if from_peaks.len() != old_accumulator.len()
      || from_peaks.len() != self.proof_paths.len()
    {
      return Err(Error::CorruptedProof);
    }

    let mut roots: Vec<M::Item> = Vec::new();
    for i in 0..from_peaks.len() {
      let root = included_root::<M>(
        from_peaks[i],
        old_accumulator[i].clone(),
        &self.proof_paths[i],
      )
      .map_err(|e| Error::MergeError(e.into()))?;
      if roots.last().is_some_and(|r| *r == root) {
        continue;
      }
      roots.push(root);
    }
    Ok(roots)
  }

  /// Verify old accumulator is consistent with new accumulator.
  pub fn verify(
    &self,
    old_accumulator: Vec<M::Item>,
    new_accumulator: &[M::Item],
  ) -> Result<bool, Error> {
    let proven = self.consistent_roots(old_accumulator)?;

    let mut idx = 0;
    for root in proven {
      if idx >= new_accumulator.len() {
        return Ok(false);
      }
      if new_accumulator[idx] == root {
        continue;
      }
      idx += 1;
      if idx >= new_accumulator.len() || new_accumulator[idx] != root {
        return Ok(false);
      }
    }
    Ok(true)
  }
}

/// Calculate root from inclusion path.
///
/// Used internally by [`InclusionProof::included_root`].
pub fn included_root<M: Merge>(
  i: u64,
  nodehash: M::Item,
  proof: &[M::Item],
) -> Result<M::Item, M::Error>
where
  M::Item: Clone,
{
  let mut root = nodehash;
  let mut g = index_height_mmriver(i);
  let mut current_i = i;

  for sibling in proof {
    if index_height_mmriver(current_i + 1) > g {
      current_i += 1;
      root = M::merge_pos(current_i + 1, sibling, &root)?;
    } else {
      current_i += 2 << g;
      root = M::merge_pos(current_i + 1, &root, sibling)?;
    }
    g += 1;
  }

  Ok(root)
}
