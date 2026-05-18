use crate::Error;
use crate::borrow::Cow;
use crate::helper::{
  consistency_proof_paths, index_height_mmriver, peaks_mmriver,
};
use crate::merge::{MergeMMRIVER, MergeResult};
use crate::mmr_store::{MMRBatch, MMRStoreReadOps, MMRStoreWriteOps};
use crate::string::String;
use crate::vec;
use crate::vec::Vec;
use core::marker::PhantomData;

pub struct MMRIVER<T, M, S> {
  mmr_size: u64,
  batch: MMRBatch<T, S>,
  _merge: PhantomData<M>,
}

impl<T, M, S> MMRIVER<T, M, S> {
  pub fn new(mmr_size: u64, store: S) -> Self {
    MMRIVER {
      mmr_size,
      batch: MMRBatch::new(store),
      _merge: PhantomData,
    }
  }

  pub fn mmr_size(&self) -> u64 {
    self.mmr_size
  }

  pub fn is_empty(&self) -> bool {
    self.mmr_size == 0
  }

  pub fn batch(&self) -> &MMRBatch<T, S> {
    &self.batch
  }

  pub fn store(&self) -> &S {
    self.batch.store()
  }
}

impl<
  T: Clone + PartialEq + Send + Sync,
  M: MergeMMRIVER<Item = T>,
  S: MMRStoreReadOps<T>,
> MMRIVER<T, M, S>
{
  async fn find_elem<'b>(
    &self,
    pos: u64,
    hashes: &'b [T],
  ) -> core::result::Result<Cow<'b, T>, Error<S::Error>> {
    let pos_offset = pos.checked_sub(self.mmr_size);
    if let Some(elem) = pos_offset.and_then(|i| hashes.get(i as usize)) {
      return Ok(Cow::Borrowed(elem));
    }

    self
      .batch
      .get_elem(pos)
      .await
      .map_err(Error::StoreError)?
      .ok_or(Error::InconsistentStore)
      .map(Cow::Owned)
  }

  pub async fn push(
    &mut self,
    elem: T,
  ) -> core::result::Result<u64, Error<S::Error>> {
    let elem_pos = self.mmr_size;
    let mut elems = vec![elem];
    let mut i = self.mmr_size + 1;
    let mut g = 0u8;

    while index_height_mmriver(i) > g {
      let left_pos = i - (2 << g);
      let right_pos = i - 1;
      let left_elem = self.find_elem(left_pos, &elems).await?;
      let right_elem = self.find_elem(right_pos, &elems).await?;
      let parent_elem = M::merge_pos(i + 1, &left_elem, &right_elem)
        .map_err(Error::MergeError)?;
      elems.push(parent_elem);
      i += 1;
      g += 1;
    }

    self.batch.append(elem_pos, elems);
    self.mmr_size = i;
    Ok(elem_pos)
  }

  pub async fn get_accumulator(
    &self,
  ) -> core::result::Result<Vec<T>, Error<S::Error>> {
    if self.mmr_size == 0 {
      return Err(Error::GetRootOnEmpty);
    }
    let peak_positions = peaks_mmriver(self.mmr_size - 1);
    let mut peaks: Vec<T> = Vec::with_capacity(peak_positions.len());
    for peak_pos in peak_positions {
      let elem = self
        .batch
        .get_elem(peak_pos)
        .await
        .map_err(Error::StoreError)?
        .ok_or(Error::InconsistentStore)?;
      peaks.push(elem);
    }
    Ok(peaks)
  }

  pub async fn get_root(&self) -> core::result::Result<T, Error<S::Error>> {
    let peaks = self.get_accumulator().await?;
    self
      .bag_rhs_peaks(peaks)
      .map_err(Error::MergeError)?
      .ok_or(Error::InconsistentStore)
  }

  fn bag_rhs_peaks(&self, mut rhs_peaks: Vec<T>) -> MergeResult<Option<T>> {
    while rhs_peaks.len() > 1 {
      let right_peak = rhs_peaks.pop().expect("pop");
      let left_peak = rhs_peaks.pop().expect("pop");
      rhs_peaks.push(M::merge_peaks(&right_peak, &left_peak)?);
    }
    Ok(rhs_peaks.pop())
  }

  pub async fn gen_consistency_proof(
    &self,
    mmr_size_from: u64,
  ) -> core::result::Result<ConsistencyProof<T, M>, Error<S::Error>> {
    if mmr_size_from == 0 || mmr_size_from > self.mmr_size {
      return Err(Error::GenProofForInvalidLeaves);
    }

    let ifrom = mmr_size_from - 1;
    let ito = self.mmr_size - 1;

    let proof_indices = consistency_proof_paths(ifrom, ito);
    let mut proof_paths: Vec<Vec<T>> = Vec::with_capacity(proof_indices.len());

    for path_indices in proof_indices {
      let mut path_values: Vec<T> = Vec::with_capacity(path_indices.len());
      for idx in path_indices {
        let elem = self
          .batch
          .get_elem(idx)
          .await
          .map_err(Error::StoreError)?
          .ok_or(Error::InconsistentStore)?;
        path_values.push(elem);
      }
      proof_paths.push(path_values);
    }

    Ok(ConsistencyProof::new(
      mmr_size_from,
      self.mmr_size,
      proof_paths,
    ))
  }

  pub async fn gen_inclusion_proof(
    &self,
    i: u64,
  ) -> core::result::Result<InclusionProof<T, M>, Error<S::Error>> {
    if i >= self.mmr_size {
      return Err(Error::GenProofForInvalidLeaves);
    }

    let c = self.mmr_size - 1;
    let path_indices = crate::helper::inclusion_proof_path(i, c);

    let mut path_values: Vec<T> = Vec::with_capacity(path_indices.len());
    for idx in path_indices {
      let elem = self
        .batch
        .get_elem(idx)
        .await
        .map_err(Error::StoreError)?
        .ok_or(Error::InconsistentStore)?;
      path_values.push(elem);
    }

    Ok(InclusionProof::new(i, path_values))
  }
}

impl<T: Send, M, S: MMRStoreWriteOps<T>> MMRIVER<T, M, S> {
  pub async fn commit(&mut self) -> core::result::Result<(), Error<S::Error>> {
    self.batch.commit().await.map_err(Error::StoreError)
  }
}

#[derive(Debug)]
pub struct InclusionProof<T, M> {
  index: u64,
  proof: Vec<T>,
  _merge: PhantomData<M>,
}

impl<T: Clone + PartialEq, M: MergeMMRIVER<Item = T>> InclusionProof<T, M> {
  pub fn new(index: u64, proof: Vec<T>) -> Self {
    InclusionProof {
      index,
      proof,
      _merge: PhantomData,
    }
  }

  pub fn index(&self) -> u64 {
    self.index
  }

  pub fn proof(&self) -> &[T] {
    &self.proof
  }

  pub fn included_root(&self, nodehash: T) -> MergeResult<T> {
    included_root::<M, T>(self.index, nodehash, &self.proof)
  }

  pub fn verify(
    &self,
    accumulator: &[T],
  ) -> core::result::Result<bool, Error<String>> {
    let root = self
      .included_root(self.proof.last().cloned().ok_or(Error::CorruptedProof)?)
      .map_err(Error::MergeError)?;

    let peak_positions = peaks_mmriver(self.index);
    if peak_positions.is_empty() {
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

#[derive(Debug)]
pub struct ConsistencyProof<T, M> {
  mmr_size_from: u64,
  mmr_size_to: u64,
  proof_paths: Vec<Vec<T>>,
  _merge: PhantomData<M>,
}

impl<T: Clone + PartialEq, M: MergeMMRIVER<Item = T>> ConsistencyProof<T, M> {
  pub fn new(
    mmr_size_from: u64,
    mmr_size_to: u64,
    proof_paths: Vec<Vec<T>>,
  ) -> Self {
    ConsistencyProof {
      mmr_size_from,
      mmr_size_to,
      proof_paths,
      _merge: PhantomData,
    }
  }

  pub fn mmr_size_from(&self) -> u64 {
    self.mmr_size_from
  }

  pub fn mmr_size_to(&self) -> u64 {
    self.mmr_size_to
  }

  pub fn proof_paths(&self) -> &[Vec<T>] {
    &self.proof_paths
  }

  pub fn consistent_roots(
    &self,
    old_accumulator: Vec<T>,
  ) -> core::result::Result<Vec<T>, Error<String>> {
    let from_peaks = peaks_mmriver(self.mmr_size_from - 1);
    if from_peaks.len() != old_accumulator.len()
      || from_peaks.len() != self.proof_paths.len()
    {
      return Err(Error::CorruptedProof);
    }

    let mut roots: Vec<T> = Vec::new();
    for i in 0..from_peaks.len() {
      let root = included_root::<M, T>(
        from_peaks[i],
        old_accumulator[i].clone(),
        &self.proof_paths[i],
      )
      .map_err(Error::MergeError)?;
      if roots.last().is_some_and(|r| *r == root) {
        continue;
      }
      roots.push(root);
    }
    Ok(roots)
  }

  pub fn verify(
    &self,
    old_accumulator: Vec<T>,
    new_accumulator: &[T],
  ) -> core::result::Result<bool, Error<String>> {
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

pub fn included_root<M: MergeMMRIVER<Item = T>, T: Clone>(
  i: u64,
  nodehash: T,
  proof: &[T],
) -> MergeResult<T> {
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
