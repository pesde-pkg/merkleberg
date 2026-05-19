use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use digest::Output;
use merkleberg::{MMRIVER, DigestMerge, util::MemStore};
use rand::{seq::SliceRandom, thread_rng};
use sha2::Sha256;
use std::sync::LazyLock;

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

fn prepare_mmriver(count: u64) -> (u64, MemStore<Output<Sha256>>, Vec<u64>) {
  RT.block_on(async {
    let store = MemStore::default();
    let mut mmr = MMRIVER::<DigestMerge<Sha256>, _>::new(0, store);
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
    for input in inputs.iter() {
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
    let mmr = MMRIVER::<DigestMerge<Sha256>, _>::new(mmr_size, store);
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
    let mmr = MMRIVER::<DigestMerge<Sha256>, _>::new(mmr_size, store.clone());
    let mut rng = thread_rng();
    let accumulator: Vec<Output<Sha256>> =
      RT.block_on(async { mmr.get_accumulator().await.unwrap() });
    let proofs: Vec<_> = RT.block_on(async {
      let mut proofs = Vec::new();
      for _ in 0..10_000 {
        let pos = positions.choose(&mut rng).unwrap();
        let proof = mmr.gen_inclusion_proof(*pos).await.unwrap();
        proofs.push((pos, proof));
      }
      proofs
    });
    b.iter(|| {
      let (_pos, proof) = proofs.choose(&mut rng).unwrap();
      proof.verify(&accumulator).unwrap();
    });
  });

  {
    let mut group = c.benchmark_group("MMRIVER consistency proof");
    let (mmr_size, store, _) = prepare_mmriver(1_000_000);
    let mmr = MMRIVER::<DigestMerge<Sha256>, _>::new(mmr_size, store);

    let sizes = [7, 15, 31, 255, 1023, 4095, 16383, 65535, 262143, 1_048_575];
    for size in sizes.iter() {
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
    let mmr = MMRIVER::<DigestMerge<Sha256>, _>::new(mmr_size, store.clone());
    let new_accumulator: Vec<Output<Sha256>> =
      RT.block_on(async { mmr.get_accumulator().await.unwrap() });

    let old_sizes = [7, 15, 31, 255, 1023, 4095, 16383];

    let proofs: Vec<_> = {
      let mut results = Vec::new();
      for from_size in old_sizes.iter() {
        let leaf_count = (*from_size + 1) / 2;
        let (old_mmr_size, old_store, _) = prepare_mmriver(leaf_count);
        let old_acc: Vec<Output<Sha256>> = RT.block_on(async {
          MMRIVER::<DigestMerge<Sha256>, _>::new(old_mmr_size, old_store)
            .get_accumulator()
            .await
            .unwrap()
        });
        let proof = RT.block_on(async {
          mmr.gen_consistency_proof(*from_size).await.unwrap()
        });
        results.push((old_acc, proof));
      }
      results
    };

    let mut rng = thread_rng();
    b.iter(|| {
      let (old_acc, proof) = proofs.choose(&mut rng).unwrap();
      proof.verify(old_acc.clone(), &new_accumulator).unwrap();
    });
  });

  c.bench_function("MMRIVER get accumulator", |b| {
    let (mmr_size, store, _) = prepare_mmriver(1_000_000);
    let mmr = MMRIVER::<DigestMerge<Sha256>, _>::new(mmr_size, store);
    b.iter(|| RT.block_on(async { mmr.get_accumulator().await }));
  });

  c.bench_function("MMRIVER get root", |b| {
    let (mmr_size, store, _) = prepare_mmriver(1_000_000);
    let mmr = MMRIVER::<DigestMerge<Sha256>, _>::new(mmr_size, store);
    b.iter(|| RT.block_on(async { mmr.get_root().await }));
  });
}

criterion_group!(
  name = benches;
  config = Criterion::default().sample_size(20);
  targets = bench
);
criterion_main!(benches);
