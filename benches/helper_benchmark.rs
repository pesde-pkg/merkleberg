#[macro_use]
extern crate criterion;
use criterion::Criterion;

use merkleberg::helper::{
  inclusion_proof_path, index_height_mmriver, leaf_index_to_mmr_size,
  leaf_index_to_pos, PeaksMMRIVERIter
};
use rand::{Rng, thread_rng};

fn bench(c: &mut Criterion) {
  c.bench_function("leaf_index_to_pos", |b| {
    let mut rng = thread_rng();
    b.iter(|| {
      let leaf_index = rng.gen_range(50_000_000_000..70_000_000_000);
      leaf_index_to_pos(leaf_index);
    });
  });

  c.bench_function("leaf_index_to_mmr_size", |b| {
    let mut rng = thread_rng();
    b.iter(|| {
      let leaf_index = rng.gen_range(50_000_000_000..70_000_000_000);
      leaf_index_to_mmr_size(leaf_index);
    });
  });

  c.bench_function("index_height_mmriver", |b| {
    let mut rng = thread_rng();
    b.iter(|| {
      let i = rng.gen_range(0..1_000_000);
      index_height_mmriver(i);
    });
  });

  c.bench_function("peaks_mmriver", |b| {
    let mut rng = thread_rng();
    b.iter(|| {
      let i = rng.gen_range(0..1_000_000);
      PeaksMMRIVERIter::new(i);
    });
  });

  c.bench_function("inclusion_proof_path (small)", |b| {
    let mut rng = thread_rng();
    b.iter(|| {
      let i = rng.gen_range(0..1000);
      let c = rng.gen_range(1000..2000);
      inclusion_proof_path(i, c);
    });
  });

  c.bench_function("inclusion_proof_path (large)", |b| {
    let mut rng = thread_rng();
    b.iter(|| {
      let i = rng.gen_range(0..100_000);
      let c = rng.gen_range(100_000..1_000_000);
      inclusion_proof_path(i, c);
    });
  });
}

criterion_group!(benches, bench);
criterion_main!(benches);
