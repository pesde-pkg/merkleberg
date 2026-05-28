use merkleberg::{
  DigestMerge, MMR, Merge as _,
  util::{MemMMR, MemStore},
};
use sha2::Sha256;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let store = MemStore::default();
  let mut mmr: MemMMR<DigestMerge<Sha256>> = MMR::new(0, store);

  let pos0 = mmr.push(b"first").await?;
  let _pos1 = mmr.push(b"second").await?;
  mmr.commit().await?;

  let root = mmr.get_root().await?;
  let proof = mmr.gen_proof(vec![pos0]).await?;
  let leaf_hash = DigestMerge::<Sha256>::leaf_hash(b"first")?;

  let is_valid = proof.verify(&root, vec![(pos0, leaf_hash)])?;
  assert!(is_valid);

  Ok(())
}
