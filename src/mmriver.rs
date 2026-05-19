use crate::Error;
use crate::borrow::Cow;
use crate::helper::{
  consistency_proof_paths, index_height_mmriver, PeaksMMRIVERIter,
};
use crate::merge::{Merge, MergeResult};
use crate::mmr_store::{MMRBatch, MMRStoreReadOps, MMRStoreWriteOps};
use crate::string::String;
use crate::vec;
use crate::vec::Vec;
use core::marker::PhantomData;

pub struct MMRIVER<M: Merge, S> {
  mmr_size: u64,
  batch: MMRBatch<M::Item, S>,
  _merge: PhantomData<M>,
}

impl<M: Merge, S> MMRIVER<M, S> {
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

  pub fn batch(&self) -> &MMRBatch<M::Item, S> {
    &self.batch
  }

  pub fn store(&self) -> &S {
    self.batch.store()
  }
}

impl<M: Merge, S: MMRStoreReadOps<M::Item>> MMRIVER<M, S>
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
    data: &[u8],
  ) -> core::result::Result<u64, Error<S::Error>> {
    let elem = M::leaf_hash(data).map_err(Error::MergeError)?;
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
  ) -> core::result::Result<Vec<M::Item>, Error<S::Error>> {
    if self.mmr_size == 0 {
      return Err(Error::GetRootOnEmpty);
    }
    let elems = self
      .batch
      .get_elems(PeaksMMRIVERIter::new(self.mmr_size - 1))
      .await
      .map_err(Error::StoreError)?;
    let peaks: Vec<M::Item> = elems
      .into_iter()
      .map(|elem| elem.ok_or(Error::InconsistentStore))
      .collect::<core::result::Result<Vec<_>, _>>()?;
    Ok(peaks)
  }

  pub async fn get_root(
    &self,
  ) -> core::result::Result<M::Item, Error<S::Error>> {
    let peaks = self.get_accumulator().await?;
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

  pub async fn gen_consistency_proof(
    &self,
    mmr_size_from: u64,
  ) -> core::result::Result<ConsistencyProof<M>, Error<S::Error>> {
    if mmr_size_from == 0 || mmr_size_from > self.mmr_size {
      return Err(Error::GenProofForInvalidLeaves);
    }

    let ifrom = mmr_size_from - 1;
    let ito = self.mmr_size - 1;

    let proof_indices = consistency_proof_paths(ifrom, ito);

    let all_elems = self
      .batch
      .get_elems(proof_indices.iter().flatten().copied())
      .await
      .map_err(Error::StoreError)?;

    let mut proof_paths: Vec<Vec<M::Item>> =
      Vec::with_capacity(proof_indices.len());
    let mut offset = 0;
    for path_indices in &proof_indices {
      let path_values: Vec<M::Item> = all_elems
        [offset..offset + path_indices.len()]
        .iter()
        .cloned()
        .map(|elem| elem.ok_or(Error::InconsistentStore))
        .collect::<core::result::Result<Vec<_>, _>>()?;
      proof_paths.push(path_values);
      offset += path_indices.len();
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
  ) -> core::result::Result<InclusionProof<M>, Error<S::Error>> {
    if i >= self.mmr_size {
      return Err(Error::GenProofForInvalidLeaves);
    }

    let c = self.mmr_size - 1;
    let path_indices = crate::helper::inclusion_proof_path(i, c);

    let elems = self
      .batch
      .get_elems(path_indices.into_iter())
      .await
      .map_err(Error::StoreError)?;
    let path_values: Vec<M::Item> = elems
      .into_iter()
      .map(|elem| elem.ok_or(Error::InconsistentStore))
      .collect::<core::result::Result<Vec<_>, _>>()?;

    Ok(InclusionProof::new(i, path_values))
  }
}

impl<M: Merge, S: MMRStoreWriteOps<M::Item>> MMRIVER<M, S>
where
  M::Item: Send,
{
  pub async fn commit(&mut self) -> core::result::Result<(), Error<S::Error>> {
    self.batch.commit().await.map_err(Error::StoreError)
  }
}

#[derive(Debug)]
pub struct InclusionProof<M: Merge> {
  index: u64,
  proof: Vec<M::Item>,
  _merge: PhantomData<M>,
}

impl<M: Merge> InclusionProof<M>
where
  M::Item: Clone + PartialEq,
{
  pub fn new(index: u64, proof: Vec<M::Item>) -> Self {
    InclusionProof {
      index,
      proof,
      _merge: PhantomData,
    }
  }

  pub fn index(&self) -> u64 {
    self.index
  }

  pub fn proof(&self) -> &[M::Item] {
    &self.proof
  }

  pub fn included_root(&self, nodehash: M::Item) -> MergeResult<M::Item> {
    included_root::<M>(self.index, nodehash, &self.proof)
  }

  pub fn verify(
    &self,
    accumulator: &[M::Item],
  ) -> core::result::Result<bool, Error<String>> {
    let root = self
      .included_root(self.proof.last().cloned().ok_or(Error::CorruptedProof)?)
      .map_err(Error::MergeError)?;

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
{
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

  pub fn mmr_size_from(&self) -> u64 {
    self.mmr_size_from
  }

  pub fn mmr_size_to(&self) -> u64 {
    self.mmr_size_to
  }

  pub fn proof_paths(&self) -> &[Vec<M::Item>] {
    &self.proof_paths
  }

  pub fn consistent_roots(
    &self,
    old_accumulator: Vec<M::Item>,
  ) -> core::result::Result<Vec<M::Item>, Error<String>> {
    let from_peaks: Vec<u64> = PeaksMMRIVERIter::new(self.mmr_size_from - 1).collect();
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
    old_accumulator: Vec<M::Item>,
    new_accumulator: &[M::Item],
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

pub fn included_root<M: Merge>(
  i: u64,
  nodehash: M::Item,
  proof: &[M::Item],
) -> MergeResult<M::Item>
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
