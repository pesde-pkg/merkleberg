# Merkleberg

[![Crates.io](https://img.shields.io/crates/v/merkleberg.svg)](https://crates.io/crates/merkleberg)

Merkleberg is a library providing asynchronous Merkle Mountain Range data structures.

## Features

- Simple bagged peaks **MMR** and accumulator based **MMRIVER** (based on the IETF draft) with consistency proofs
- Fully asynchronous API without reliance on any specific runtime
- Fully extensible with custom hashing via `Merge` trait and custom storage via other traits
- Protects against second preimage attacks by default
- Compatible with `no_std`, just disable the `std` feature

| Feature           | MMR                        | MMRIVER                     |
| ----------------- | -------------------------- | --------------------------- |
| Root              | Single hash (bagged peaks) | List of peaks (accumulator) |
| Inclusion proof   | ✓                          | ✓                           |
| Multi-leaf proof  | ✓                          | ✗ (single leaf only)        |
| Consistency proof | ✗                          | ✓                           |
| IETF spec         | OpenTimestamps             | [IETF MMRIVER Draft]        |

## Core Concepts

### MMR

An MMR is a series of complete binary trees ("mountains") stored in post-order traversal:

```text
# An 11 leaves MMR

          14
       /       \
     6          13
   /   \       /   \
  2     5     9     12     17
 / \   /  \  / \   /  \   /  \
0   1 3   4 7   8 10  11 15  16 18
```

Nodes are indexed by insertion order. To add a leaf:

1. Append leaf at next position
2. If position has left sibling at same height, merge them into parent node
3. Repeat step 2 until no more merging possible

```rust,ignore
use merkleberg::{MMR, DigestMerge, util::MemStore, Merge};
use sha2::Sha256;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create MMR with SHA-256 hashing
    let store = MemStore::default();
    let mut mmr: MMR<DigestMerge<Sha256>, _> = MMR::new(0, store);

    // Add elements
    let pos0 = mmr.push(b"first").await?;
    let pos1 = mmr.push(b"second").await?;

    // Persist to storage
    mmr.commit().await?;

    // Compute Merkle root
    let root = mmr.get_root().await?;

    // Generate inclusion proof
    let proof = mmr.gen_proof(vec![pos0]).await?;

    // Verify proof
    let leaf_hash = DigestMerge::<Sha256>::leaf_hash(b"first")?;
    assert!(proof.verify(root, vec![(pos0, leaf_hash)])?);

    Ok(())
}
```

### MMRIVER

MMRIVER (Merkle Mountain Range for Immediately Verifiable and Replicable Commitments) is an experimental
draft IETF RFC. It differs from regular MMR by storing a list (i.e., the accumulator) of peaks instead of
bagging from right to left into a single hash.

This approach allows for **consistency proofs** alongside the existing inclusion proofs already supported by
MMR, where any new accumulator can be verified against an existing old accumulator (also known as the [*Reyzin-Yakoubov*]
property, which is useful for **blockchain header chains**, **verifiable log replication**, **state evolution proofs**,
etc.

[*ReyzinYakoubov*]: https://eprint.iacr.org/2015/718.pdf

```rust,ignore
use merkleberg::{MMRIVER, DigestMerge, util::MemStore, Merge};
use sha2::Sha256;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemStore::default();
    let mut mmriver: MMRIVER<DigestMerge<Sha256>, _> = MMRIVER::new(0, store);

    // Add elements
    for i in 0u64..10 {
        mmriver.push(&i.to_be_bytes()).await?;
    }
    mmriver.commit().await?;

    // Get accumulator (list of peaks) instead of single root
    let accumulator = mmriver.get_accumulator().await?;

    // Generate inclusion proof
    let proof = mmriver.gen_inclusion_proof(0).await?;
    let leaf_hash = DigestMerge::<Sha256>::leaf_hash(&0u64.to_be_bytes())?;
    assert!(proof.verify(leaf_hash, &accumulator)?);

    // Generate consistency proof between tree states
    let old_size = mmriver.mmr_size();
    for i in 10u64..20 {
        mmriver.push(&i.to_be_bytes()).await?;
    }
    mmriver.commit().await?;

    let consistency_proof = mmriver.gen_consistency_proof(old_size).await?;
    let new_accumulator = mmriver.get_accumulator().await?;
    assert!(consistency_proof.verify(accumulator, &new_accumulator)?);

    Ok(())
}
```

### The `Merge` Trait

Merkleberg does not specify any hashing logic internally. Hashing logic is specified by implementing the
`Merge` trait.

```rust,ignore
use merkleberg::Merge;
use std::convert::Infallible;

struct MyMerge;

impl Merge for MyMerge {
    type Item = [u8; 32];  // Hash output type
    type Error = Infallible;

    fn leaf_hash(data: &[u8]) -> Result<Self::Item, Self::Error> {
        // Hash leaf with domain prefix (e.g., 0x00)
    }

    fn merge_pos(pos: u64, left: &Self::Item, right: &Self::Item)
        -> Result<Self::Item, Self::Error> {
        // Hash node with position prefix for domain separation
    }
}
```

However, a `DigestMerge` generic type is provided for convenience which provides a simple implementation without
specifying a hashing algorithm. Any algorithm implementing the [`Digest`] trait can be supplied to it. Common
hashing algorithms can be found as a part of the [RustCrypto] project.

By default, `DigestMerge` uses domain prefixes to prevent [second preimage attacks]:

- Leaves: prefixed with `0x00`
- Nodes: prefixed with `0x01`

This ensures a leaf hash cannot be crafted to match a node hash.

Should you choose to opt-out of this much recommended protection, the `unsafe-digest` feature
may be enabled to make use of `DigestMergeUnsafe` instead.

[`Digest`]: https://docs.rs/digest/latest/digest/trait.Digest.html
[RustCrypto]: https://github.com/RustCrypto/hashes
[second preimage attacks]: https://en.wikipedia.org/wiki/Merkle_tree#Second_preimage_attack

### Custom Storage Backends

Implement `MMRStoreReadOps` and `MMRStoreWriteOps`:

```rust,ignore
use merkleberg::{MMRStoreReadOps, MMRStoreWriteOps};

struct MyStore {
    // Your storage backend (database, file, memory, etc.)
}

impl<T: Clone + Send + Sync> MMRStoreReadOps<T> for MyStore {
    type Error = MyError;

    async fn get_elem(&self, pos: u64) -> Result<Option<T>, Self::Error>;
    async fn get_elems(&self, positions: impl Iterator<Item = u64> + Send)
        -> Result<Vec<Option<T>>, Self::Error>;
}

impl<T: Send + Sync> MMRStoreWriteOps<T> for MyStore {
    type Error = MyError;

    async fn append(&mut self, pos: u64, elems: Vec<T>) -> Result<(), Self::Error>;
}
```

For convenience, a lightweight `MemStore` backend is provided by default, which stores trees
in memory. Storage backends also support batching via `MMRBatch`; multiple items can be pushed
before committing and actually updating the underlying structure.

## Error Handling

Errors use trait objects for flexibility:

```rust,ignore
type UserError = Box<dyn Error + Send + Sync + 'static>;

pub enum Error {
    StoreError(UserError),
    MergeError(UserError),
    // ...
}
```

Custom store / merge errors need only implement `Error + Send + Sync + 'static`.

## References

- [OpenTimestamps MMR spec](https://github.com/opentimestamps/opentimestamps-server/blob/master/doc/merkle-mountain-range.md)
- [Grin documentation](https://github.com/mimblewimble/grin/blob/master/doc/mmr.md)
- [Nervos implementation](https://github.com/nervosnetwork/merkle-mountain-range)
- [IETF MMRIVER Draft] - Merkle Mountain Range for Immediately Verifiable and Replicable Commitments
- [Wikipedia: Merkle tree](https://en.wikipedia.org/wiki/Merkle_tree)

[IETF MMRIVER Draft]: https://github.com/robinbryce/merkle-mountain-range-proofs/blob/8c013b9777ceacb70873172ad142042f01294d41/draft-bryce-cose-merkle-mountain-range-proofs-00.md

## License

This project is dual-licensed under:

- MIT License ([original](https://github.com/nervosnetwork/merkle-mountain-range/blob/c0c9263122a3901ea3c3e716e1c1faec1e592ff4/LICENSE))
- Mozilla Public License 2.0 - MMRIVER and other modifications made by [@pesde-pkg](https://github.com/pesde-pkg)

See [LICENSE](https://github.com/pesde-pkg/merkleberg/blob/main/LICENSE) for full details.
