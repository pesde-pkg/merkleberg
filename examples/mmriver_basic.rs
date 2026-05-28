use merkleberg::{
  DigestMerge, MMRIVER, Merge as _,
  util::{MemMMRIVER, MemStore},
};
use sha2::Sha256;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let store = MemStore::default();
  let mut mmriver: MemMMRIVER<DigestMerge<Sha256>> = MMRIVER::new(0, store);
  for i in 0u64..10 {
    mmriver.push(&i.to_be_bytes()).await?;
  }
  mmriver.commit().await?;
  let accumulator = mmriver.get_accumulator().await?;
  let proof = mmriver.gen_inclusion_proof(0).await?;
  let leaf_hash = DigestMerge::<Sha256>::leaf_hash(&0u64.to_be_bytes())?;
  let is_valid = proof.verify(leaf_hash, &accumulator)?;
  assert!(is_valid);

  let old_size = mmriver.mmr_size();
  let old_accumulator = accumulator;
  for i in 10u64..20 {
    mmriver.push(&i.to_be_bytes()).await?;
  }
  mmriver.commit().await?;
  let new_accumulator = mmriver.get_accumulator().await?;
  let consistency_proof = mmriver.gen_consistency_proof(old_size).await?;
  let is_consistent =
    consistency_proof.verify(&old_accumulator, &new_accumulator)?;
  assert!(is_consistent);

  Ok(())
}
