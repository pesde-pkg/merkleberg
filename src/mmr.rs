//! Merkle Mountain Range
//!
//! references:
//! https://github.com/mimblewimble/grin/blob/master/doc/mmr.md#structure
//! https://github.com/mimblewimble/grin/blob/0ff6763ee64e5a14e70ddd4642b99789a1648a32/core/src/core/pmmr.rs#L606

use crate::Error;
use crate::borrow::Cow;
use crate::collections::VecDeque;
use crate::helper::{
  get_peak_map, get_peaks, leaf_index_to_mmr_size, leaf_index_to_pos,
  parent_offset, pos_height_in_tree, sibling_offset,
};
use crate::merge::{Merge, MergeResult};
use crate::mmr_store::{MMRBatch, MMRStoreReadOps, MMRStoreWriteOps};
use crate::string::String;
use crate::vec;
use crate::vec::Vec;
use core::fmt::Debug;
use core::marker::PhantomData;
use core::mem;

#[allow(clippy::upper_case_acronyms)]
pub struct MMR<M: Merge, S> {
  mmr_size: u64,
  batch: MMRBatch<M::Item, S>,
  merge: PhantomData<M>,
}

impl<M: Merge, S> MMR<M, S> {
  pub fn new(mmr_size: u64, store: S) -> Self {
    MMR {
      mmr_size,
      batch: MMRBatch::new(store),
      merge: PhantomData,
    }
  }

  pub fn mmr_size(&self) -> u64 {
    self.mmr_size
  }

  pub fn is_empty(&self) -> bool {
    self.mmr_size == 0
  }

  pub fn batch(&self) -> &MMRBatch<M::Item, S> {
    &self.batch
  }

  pub fn store(&self) -> &S {
    self.batch.store()
  }
}

impl<M: Merge, S: MMRStoreReadOps<M::Item>> MMR<M, S>
where
  M::Item: Clone + PartialEq + Send + Sync,
{
  async fn find_elem<'b>(
    &self,
    pos: u64,
    hashes: &'b [M::Item],
  ) -> core::result::Result<Cow<'b, M::Item>, Error<S::Error>> {
    let pos_offset = pos.checked_sub(self.mmr_size);
    if let Some(elem) = pos_offset.and_then(|i| hashes.get(i as usize)) {
      return Ok(Cow::Borrowed(elem));
    }
    let elem = self
      .batch
      .get_elem(pos)
      .await
      .map_err(Error::StoreError)?
      .ok_or(Error::InconsistentStore)?;
    Ok(Cow::Owned(elem))
  }

  pub async fn push(
    &mut self,
    data: &[u8],
  ) -> core::result::Result<u64, Error<S::Error>> {
    let elem = M::leaf_hash(data).map_err(Error::MergeError)?;
    let mut elems = vec![elem];
    let elem_pos = self.mmr_size;
    let peak_map = get_peak_map(self.mmr_size);
    let mut pos = self.mmr_size;
    let mut peak = 1;
    while (peak_map & peak) != 0 {
      peak <<= 1;
      pos += 1;
      let left_pos = pos - peak;
      let left_elem = self.find_elem(left_pos, &elems).await?;
      let right_elem = elems.last().expect("checked");
      let parent_elem =
        M::merge(&left_elem, right_elem).map_err(Error::MergeError)?;
      elems.push(parent_elem);
    }
    self.batch.append(elem_pos, elems);
    self.mmr_size = pos + 1;
    Ok(elem_pos)
  }

  pub async fn get_root(
    &self,
  ) -> core::result::Result<M::Item, Error<S::Error>> {
    if self.mmr_size == 0 {
      return Err(Error::GetRootOnEmpty);
    } else if self.mmr_size == 1 {
      return self
        .batch
        .get_elem(0)
        .await
        .map_err(Error::StoreError)?
        .ok_or(Error::InconsistentStore);
    }
    let peak_positions = get_peaks(self.mmr_size);
    let mut peaks: Vec<M::Item> = Vec::with_capacity(peak_positions.len());
    for peak_pos in peak_positions {
      let elem = self
        .batch
        .get_elem(peak_pos)
        .await
        .map_err(Error::StoreError)?
        .ok_or(Error::InconsistentStore)?;
      peaks.push(elem);
    }
    self
      .bag_rhs_peaks(peaks)
      .map_err(Error::MergeError)?
      .ok_or(Error::InconsistentStore)
  }

  fn bag_rhs_peaks(
    &self,
    mut rhs_peaks: Vec<M::Item>,
  ) -> MergeResult<Option<M::Item>> {
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
  ) -> core::result::Result<(), Error<S::Error>> {
    if pos_list.len() == 1 && pos_list == [peak_pos] {
      return Ok(());
    }
    if pos_list.is_empty() {
      proof.push(
        self
          .batch
          .get_elem(peak_pos)
          .await
          .map_err(Error::StoreError)?
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
        } else {
          return Err(Error::NodeProofsNotSupported);
        }
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
            .map_err(Error::StoreError)?
            .ok_or(Error::InconsistentStore)?,
        );
      }
      if parent_pos < peak_pos {
        queue.push_back((parent_pos, height + 1));
      }
    }
    Ok(())
  }

  pub async fn gen_proof(
    &self,
    mut pos_list: Vec<u64>,
  ) -> core::result::Result<InclusionProof<M>, Error<S::Error>> {
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
    let peaks = get_peaks(self.mmr_size);
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
          .map_err(Error::MergeError)?
          .expect("bagging rhs peaks"),
      );
    }

    Ok(InclusionProof::new(self.mmr_size, proof))
  }
}

impl<M: Merge, S: MMRStoreWriteOps<M::Item>> MMR<M, S>
where
  M::Item: Send,
{
  pub async fn commit(&mut self) -> core::result::Result<(), Error<S::Error>> {
    self.batch.commit().await.map_err(Error::StoreError)
  }
}

#[derive(Debug)]
pub struct InclusionProof<M: Merge> {
  mmr_size: u64,
  proof: Vec<M::Item>,
  merge: PhantomData<M>,
}

impl<M: Merge> InclusionProof<M>
where
  M::Item: Clone + PartialEq,
{
  pub fn new(mmr_size: u64, proof: Vec<M::Item>) -> Self {
    InclusionProof {
      mmr_size,
      proof,
      merge: PhantomData,
    }
  }

  pub fn mmr_size(&self) -> u64 {
    self.mmr_size
  }

  pub fn proof_items(&self) -> &[M::Item] {
    &self.proof
  }

  pub fn calculate_root(
    &self,
    leaves: Vec<(u64, M::Item)>,
  ) -> MergeResult<M::Item> {
    calculate_root::<M, _>(leaves, self.mmr_size, self.proof.iter())
  }

  pub fn calculate_root_with_new_leaf(
    &self,
    mut leaves: Vec<(u64, M::Item)>,
    new_pos: u64,
    new_elem: M::Item,
    new_mmr_size: u64,
  ) -> MergeResult<M::Item> {
    let pos_height = pos_height_in_tree(new_pos);
    let next_height = pos_height_in_tree(new_pos + 1);
    if next_height > pos_height {
      let mut peaks_hashes = calculate_peaks_hashes::<M, _>(
        leaves,
        self.mmr_size,
        self.proof.iter(),
      )?;
      let peaks_pos = get_peaks(new_mmr_size);
      let mut i = 0;
      while peaks_pos[i] < new_pos {
        i += 1
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

  pub fn verify(
    &self,
    root: M::Item,
    leaves: Vec<(u64, M::Item)>,
  ) -> MergeResult<bool> {
    self
      .calculate_root(leaves)
      .map(|calculated_root| calculated_root == root)
  }

  pub fn verify_incremental(
    &self,
    root: M::Item,
    prev_root: M::Item,
    incremental: Vec<M::Item>,
  ) -> core::result::Result<bool, Error<String>> {
    let current_leaves_count = get_peak_map(self.mmr_size);
    if current_leaves_count <= incremental.len() as u64 {
      return Err(Error::CorruptedProof);
    }
    let prev_leaves_count = current_leaves_count - incremental.len() as u64;
    let prev_peaks_positions = {
      let prev_index = prev_leaves_count - 1;
      let prev_mmr_size = leaf_index_to_mmr_size(prev_index);
      let prev_peaks_positions = get_peaks(prev_mmr_size);
      if prev_peaks_positions.len() != self.proof.len() {
        return Err(Error::CorruptedProof);
      }
      prev_peaks_positions
    };
    let current_peaks_positions = get_peaks(self.mmr_size);

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

    let calculated_prev_root =
      bagging_peaks_hashes::<M>(prev_peaks).map_err(Error::MergeError)?;
    if calculated_prev_root != prev_root {
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
    self.verify(root, leaves).map_err(Error::MergeError)
  }
}

fn calculate_peak_root<'a, M: Merge, I: Iterator<Item = &'a M::Item>>(
  leaves: Vec<(u64, M::Item)>,
  peak_pos: u64,
  proof_iter: &mut I,
) -> MergeResult<M::Item>
where
  M::Item: 'a + Clone,
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
      } else {
        return Err("Corrupted proof".into());
      }
    }
    let next_height = pos_height_in_tree(pos + 1);
    let (parent_pos, parent_item) = {
      let sibling_offset = sibling_offset(height);
      if next_height > height {
        let sib_pos = pos - sibling_offset;
        let parent_pos = pos + 1;
        let parent_item =
          if Some(&sib_pos) == queue.front().map(|(pos, _, _)| pos) {
            let sibling_item =
              queue.pop_front().map(|(_, item, _)| item).unwrap();
            M::merge(&sibling_item, &item)?
          } else {
            let sibling_item = proof_iter.next().ok_or("Corrupted proof")?;
            M::merge(sibling_item, &item)?
          };
        (parent_pos, parent_item)
      } else {
        let sib_pos = pos + sibling_offset;
        let parent_pos = pos + parent_offset(height);
        let parent_item =
          if Some(&sib_pos) == queue.front().map(|(pos, _, _)| pos) {
            let sibling_item =
              queue.pop_front().map(|(_, item, _)| item).unwrap();
            M::merge(&item, &sibling_item)?
          } else {
            let sibling_item = proof_iter.next().ok_or("Corrupted proof")?;
            M::merge(&item, sibling_item)?
          };
        (parent_pos, parent_item)
      }
    };

    if parent_pos <= peak_pos {
      queue.push_back((parent_pos, parent_item, height + 1))
    } else {
      return Err("Corrupted proof".into());
    }
  }
  Err("Corrupted proof".into())
}

fn calculate_peaks_hashes<'a, M: Merge, I: Iterator<Item = &'a M::Item>>(
  mut leaves: Vec<(u64, M::Item)>,
  mmr_size: u64,
  mut proof_iter: I,
) -> MergeResult<Vec<M::Item>>
where
  M::Item: 'a + Clone,
{
  if leaves.iter().any(|(pos, _)| pos_height_in_tree(*pos) > 0) {
    return Err("Node proofs not supported".into());
  }

  if mmr_size == 1 && leaves.len() == 1 && leaves[0].0 == 0 {
    return Ok(leaves.into_iter().map(|(_pos, item)| item).collect());
  }
  leaves.sort_by_key(|(pos, _)| *pos);
  leaves.dedup_by(|a, b| a.0 == b.0);
  let peaks = get_peaks(mmr_size);

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
    return Err("Corrupted proof".into());
  }

  if let Some(rhs_peaks_hashes) = proof_iter.next() {
    peaks_hashes.push(rhs_peaks_hashes.clone());
  }
  if proof_iter.next().is_some() {
    return Err("Corrupted proof".into());
  }
  Ok(peaks_hashes)
}

fn bagging_peaks_hashes<M: Merge>(
  mut peaks_hashes: Vec<M::Item>,
) -> MergeResult<M::Item> {
  while peaks_hashes.len() > 1 {
    let right_peak = peaks_hashes.pop().expect("pop");
    let left_peak = peaks_hashes.pop().expect("pop");
    peaks_hashes.push(M::merge_peaks(&right_peak, &left_peak)?);
  }
  peaks_hashes.pop().ok_or("Corrupted proof".into())
}

fn calculate_root<'a, M: Merge, I: Iterator<Item = &'a M::Item>>(
  leaves: Vec<(u64, M::Item)>,
  mmr_size: u64,
  proof_iter: I,
) -> MergeResult<M::Item>
where
  M::Item: 'a + Clone,
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
