#[macro_use]
extern crate criterion;

use criterion::{BenchmarkId, Criterion};

use bytes::Bytes;
use merkleberg::{Error, MMR, MMRStoreReadOps, Merge, util::MemStore};
use rand::{seq::SliceRandom, thread_rng};
use std::convert::{Infallible, TryFrom};

use blake2b_rs::{Blake2b, Blake2bBuilder};

fn new_blake2b() -> Blake2b {
  Blake2bBuilder::new(32).build()
}

#[derive(Eq, PartialEq, Clone, Debug, Default)]
struct NumberHash(pub Bytes);
impl TryFrom<u32> for NumberHash {
  type Error = Error;
  fn try_from(num: u32) -> Result<Self, Error> {
    let mut hasher = new_blake2b();
    let mut hash = [0u8; 32];
    hasher.update(&num.to_le_bytes());
    hasher.finalize(&mut hash);
    Ok(NumberHash(hash.to_vec().into()))
  }
}

struct MergeNumberHash;

impl Merge for MergeNumberHash {
  type Item = NumberHash;
  type Error = Infallible;

  fn leaf_hash(data: &[u8]) -> Result<Self::Item, Self::Error> {
    let mut hasher = new_blake2b();
    let mut hash = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut hash);
    Ok(NumberHash(hash.to_vec().into()))
  }

  fn merge_pos(
    _pos: u64,
    lhs: &Self::Item,
    rhs: &Self::Item,
  ) -> Result<Self::Item, Self::Error> {
    let mut hasher = new_blake2b();
    let mut hash = [0u8; 32];
    hasher.update(&lhs.0);
    hasher.update(&rhs.0);
    hasher.finalize(&mut hash);
    Ok(NumberHash(hash.to_vec().into()))
  }
}

fn prepare_mmr(count: u32) -> (u64, MemStore<NumberHash>, Vec<u64>) {
  let rt = tokio::runtime::Runtime::new().unwrap();
  rt.block_on(async {
    let store = MemStore::default();
    let mut mmr = MMR::<MergeNumberHash, _>::new(0, store);
    let mut positions: Vec<u64> = Vec::new();
    for i in 0u32..count {
      let pos = mmr.push(&i.to_le_bytes()).await.unwrap();
      positions.push(pos);
    }
    let mmr_size = mmr.mmr_size();
    mmr.commit().await.expect("write to store");
    (mmr_size, mmr.store().clone(), positions)
  })
}

fn bench(c: &mut Criterion) {
  {
    let mut group = c.benchmark_group("MMR insertion");
    let inputs = [10_000, 100_000, 1_000_000];
    for input in inputs.iter() {
      group.bench_with_input(
        BenchmarkId::new("times", input),
        &input,
        |b, &&size| {
          b.iter(|| prepare_mmr(size));
        },
      );
    }
  }

  c.bench_function("MMR gen proof", |b| {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (mmr_size, store, positions) = prepare_mmr(1_000_000);
    let mmr = MMR::<MergeNumberHash, _>::new(mmr_size, store);
    let mut rng = thread_rng();
    b.iter(|| {
      rt.block_on(async {
        mmr
          .gen_proof(vec![*positions.choose(&mut rng).unwrap()])
          .await
      })
    });
  });

  c.bench_function("MMR verify", |b| {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (mmr_size, store, positions) = prepare_mmr(1_000_000);
    let mmr = MMR::<MergeNumberHash, _>::new(mmr_size, store.clone());
    let mut rng = thread_rng();
    let root: NumberHash = rt.block_on(async { mmr.get_root().await.unwrap() });
    let proofs: Vec<_> = rt.block_on(async {
      let mut proofs = Vec::new();
      for _ in 0..10_000 {
        let pos = positions.choose(&mut rng).unwrap();
        let elem = store.get_elem(*pos).await.unwrap().unwrap();
        let proof = mmr.gen_proof(vec![*pos]).await.unwrap();
        proofs.push((pos, elem, proof));
      }
      proofs
    });
    b.iter(|| {
      let (pos, elem, proof) = proofs.choose(&mut rng).unwrap();
      proof
        .verify(root.clone(), vec![(**pos, elem.clone())])
        .unwrap();
    });
  });
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = bench
);
criterion_main!(benches);
