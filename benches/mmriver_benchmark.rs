use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use merkleberg::{MMRIVER, Merge, util::MemStore};
use rand::{seq::SliceRandom as _, thread_rng};
use std::sync::LazyLock;

use blake2b_rs::{Blake2b, Blake2bBuilder};
use bytes::Bytes;

fn new_blake2b() -> Blake2b {
  Blake2bBuilder::new(32).build()
}

#[derive(Eq, PartialEq, Clone, Debug, Default)]
struct NumberHash(pub Bytes);

impl From<u32> for NumberHash {
  fn from(num: u32) -> Self {
    let mut hasher = new_blake2b();
    let mut hash = [0u8; 32];
    hasher.update(&num.to_le_bytes());
    hasher.finalize(&mut hash);
    NumberHash(hash.to_vec().into())
  }
}

struct MergeNumberHash;

impl Merge for MergeNumberHash {
  type Item = NumberHash;
  type Error = std::convert::Infallible;

  fn leaf_hash(data: &[u8]) -> Result<Self::Item, Self::Error> {
    let mut hasher = new_blake2b();
    let mut hash = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut hash);
    Ok(NumberHash(hash.to_vec().into()))
  }

  fn merge_pos(
    _pos: u64,
    left: &Self::Item,
    right: &Self::Item,
  ) -> Result<Self::Item, Self::Error> {
    let mut hasher = new_blake2b();
    let mut hash = [0u8; 32];
    hasher.update(&left.0);
    hasher.update(&right.0);
    hasher.finalize(&mut hash);
    Ok(NumberHash(hash.to_vec().into()))
  }
}

static RT: LazyLock<tokio::runtime::Runtime> =
  LazyLock::new(|| tokio::runtime::Runtime::new().unwrap());

fn leaf_index_to_mmr_index(e: u64) -> u64 {
  let mut sum = 0u64;
  let mut e = e;
  while e > 0 {
    let h = 64 - e.leading_zeros();
    sum += (1 << h) - 1;
    let half = 1 << (h - 1);
    e -= half;
  }
  sum
}

fn prepare_mmriver(count: u64) -> (u64, MemStore<NumberHash>, Vec<u64>) {
  RT.block_on(async {
    let store = MemStore::default();
    let mut mmr = MMRIVER::<MergeNumberHash, _>::new(0, store);
    let mut positions: Vec<u64> = Vec::new();
    for e in 0..count {
      let i = leaf_index_to_mmr_index(e);
      let pos = mmr.push(&i.to_be_bytes()).await.unwrap();
      positions.push(pos);
    }
    let mmr_size = mmr.mmr_size();
    mmr.commit().await.expect("write to store");
    (mmr_size, mmr.store().clone(), positions)
  })
}

fn bench(c: &mut Criterion) {
  {
    let mut group = c.benchmark_group("MMRIVER insertion");
    let inputs = [10_000, 100_000, 1_000_000];
    for input in &inputs {
      group.bench_with_input(
        BenchmarkId::new("times", input),
        &input,
        |b, &&size| {
          b.iter(|| prepare_mmriver(size));
        },
      );
    }
  }

  c.bench_function("MMRIVER gen inclusion proof", |b| {
    let (mmr_size, store, positions) = prepare_mmriver(1_000_000);
    let mmr = MMRIVER::<MergeNumberHash, _>::new(mmr_size, store);
    let mut rng = thread_rng();
    b.iter(|| {
      RT.block_on(async {
        mmr
          .gen_inclusion_proof(*positions.choose(&mut rng).unwrap())
          .await
      })
    });
  });

  c.bench_function("MMRIVER verify inclusion proof", |b| {
    let (mmr_size, store, positions) = prepare_mmriver(1_000_000);
    let mmr = MMRIVER::<MergeNumberHash, _>::new(mmr_size, store);
    let mut rng = thread_rng();
    let accumulator: Vec<NumberHash> =
      RT.block_on(async { mmr.get_accumulator().await.unwrap() });
    let proofs: Vec<_> = RT.block_on(async {
      let mut proofs = Vec::new();
      for _ in 0i16..10_000 {
        let pos = positions.choose(&mut rng).unwrap();
        let proof = mmr.gen_inclusion_proof(*pos).await.unwrap();
        #[allow(clippy::integer_division)]
        let i = leaf_index_to_mmr_index(*pos as u64 / 2);
        let leaf_hash = MergeNumberHash::leaf_hash(&i.to_be_bytes()).unwrap();
        proofs.push((pos, proof, leaf_hash));
      }
      proofs
    });
    b.iter(|| {
      let (_pos, proof, leaf_hash) = proofs.choose(&mut rng).unwrap();
      proof.verify(leaf_hash.clone(), &accumulator).unwrap()
    });
  });

  {
    let mut group = c.benchmark_group("MMRIVER consistency proof");
    let (mmr_size, store, _) = prepare_mmriver(1_000_000);
    let mmr = MMRIVER::<MergeNumberHash, _>::new(mmr_size, store);

    let sizes = [7, 15, 31, 255, 1023, 4095, 16383, 65535, 262_143, 1_048_575];
    for size in &sizes {
      group.bench_with_input(
        BenchmarkId::new("gen from mmr_size", size),
        &size,
        |b, &&from_size| {
          b.iter(|| {
            RT.block_on(async { mmr.gen_consistency_proof(from_size).await })
          });
        },
      );
    }
  }

  c.bench_function("MMRIVER verify consistency proof", |b| {
    let (mmr_size, store, _) = prepare_mmriver(1_000_000);
    let mmr = MMRIVER::<MergeNumberHash, _>::new(mmr_size, store);
    let new_accumulator: Vec<NumberHash> =
      RT.block_on(async { mmr.get_accumulator().await.unwrap() });

    let old_sizes: [u64; 7] = [7, 15, 31, 255, 1023, 4095, 16383];

    let proofs: Vec<_> = {
      let mut results = Vec::new();
      for &from_size in &old_sizes {
        let leaf_count = from_size.div_ceil(2);
        let (old_mmr_size, old_store, _) = prepare_mmriver(leaf_count);
        let old_acc: Vec<NumberHash> = RT.block_on(async {
          MMRIVER::<MergeNumberHash, _>::new(old_mmr_size, old_store)
            .get_accumulator()
            .await
            .unwrap()
        });
        let proof = RT.block_on(async {
          mmr.gen_consistency_proof(from_size).await.unwrap()
        });
        results.push((old_acc, proof));
      }
      results
    };

    let mut rng = thread_rng();
    b.iter(|| {
      let (old_acc, proof) = proofs.choose(&mut rng).unwrap();
      proof.verify(old_acc, &new_accumulator).unwrap()
    });
  });

  c.bench_function("MMRIVER get accumulator", |b| {
    let (mmr_size, store, _) = prepare_mmriver(1_000_000);
    let mmr = MMRIVER::<MergeNumberHash, _>::new(mmr_size, store);
    b.iter(|| RT.block_on(async { mmr.get_accumulator().await }));
  });

  c.bench_function("MMRIVER get root", |b| {
    let (mmr_size, store, _) = prepare_mmriver(1_000_000);
    let mmr = MMRIVER::<MergeNumberHash, _>::new(mmr_size, store);
    b.iter(|| RT.block_on(async { mmr.get_root().await }));
  });
}

criterion_group!(
  name = benches;
  config = Criterion::default().sample_size(20);
  targets = bench
);
criterion_main!(benches);
