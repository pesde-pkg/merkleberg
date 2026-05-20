use core::{convert::Infallible, error::Error};

/// Trait for defining hash operations in an MMR.
///
/// Implement this to customize how elements are hashed and merged.
/// The trait handles:
///
/// - Leaf hashing (from raw data)
/// - Node merging (combining children)
/// - Peak merging (for root computation)
///
/// ## Security
///
/// For cryptographic security, implementations should use **domain separation**
/// to prevent [second preimage attacks]. This means:
///
/// - Leaf hashes should use a different prefix than node hashes
/// - Node hashes should include the position to prevent collisions
///
/// [second preimage attacks]: https://en.wikipedia.org/wiki/Merkle_tree#Second_preimage_attack
///
/// ## Examples
///
/// [`DigestMerge`] is the default implementation of this trait, which provides
/// domain separation as well, just provide it a hasher of your choice. For example,
/// for SHA-256:
///
/// ```rust,ignore
/// use merkleberg::{MMR, DigestMerge, util::MemStore};
/// use sha2::Sha256;
///
/// type MyMMR = MMR<DigestMerge<Sha256>, MemStore<_>>;
/// ```
///
/// ```rust,ignore
/// use merkleberg::Merge;
///
/// struct CustomMerge;
///
/// impl Merge for CustomMerge {
///     type Item = [u8; 32];
///     type Error = std::convert::Infallible;
///
///     fn leaf_hash(data: &[u8]) -> Result<Self::Item, Self::Error> {
///         // Hash with leaf domain prefix (e.g., 0x00)
///     }
///
///     fn merge_pos(pos: u64, left: &Self::Item, right: &Self::Item) 
///         -> Result<Self::Item, Self::Error> {
///         // Hash with node domain prefix (e.g., 0x01 + pos)
///     }
/// }
/// ```
///
/// ## References
///
/// - [OpenTimestamps MMR spec](https://github.com/opentimestamps/opentimestamps-server/blob/master/doc/merkle-mountain-range.md)
/// - [Wikipedia: Second preimage attack](https://en.wikipedia.org/wiki/Merkle_tree#Second_preimage_attack)
pub trait Merge {
  /// The element type stored in the MMR.
  ///
  /// Typically a hash output (e.g., `[u8; 32]` for SHA-256).
  type Item: Clone + PartialEq;

  /// Error type for merge operations.
  ///
  /// For infallible implementations (like `DigestMerge`), use [`Infallible`].
  type Error: Error + Send + Sync + 'static;

  /// Hash a leaf from raw data.
  ///
  /// ## Security
  ///
  /// Should use domain separation (e.g., prefix with `0x00`) to prevent
  /// second preimage attacks.
  fn leaf_hash(data: &[u8]) -> Result<Self::Item, Self::Error>;

  /// Merge two child nodes at a given position.
  ///
  /// ## Parameters
  ///
  /// - `pos`: The position of the parent node in the MMR
  /// - `left`: The left child element
  /// - `right`: The right child element
  ///
  /// ## Security
  ///
  /// Should include the position in the hash to prevent collision attacks.
  fn merge_pos(
    pos: u64,
    left: &Self::Item,
    right: &Self::Item,
  ) -> Result<Self::Item, Self::Error>;

  /// Merge two nodes without position context.
  ///
  /// Default implementation calls `merge_pos(0, left, right)`.
  fn merge(left: &Self::Item, right: &Self::Item) -> Result<Self::Item, Self::Error> {
    Self::merge_pos(0, left, right)
  }

  /// Merge peaks during root computation.
  ///
  /// Peaks are merged right-to-left (bagging). Default implementation
  /// uses [`Self::merge`].
  fn merge_peaks(
    left: &Self::Item,
    right: &Self::Item,
  ) -> Result<Self::Item, Self::Error> {
    Self::merge(left, right)
  }
}

cfg_if::cfg_if! {
  if #[cfg(feature = "digest")] {
    use core::marker::PhantomData;
    use digest::Digest;

    const LEAF_DOMAIN_PREFIX: u8 = 0x00;
    const NODE_DOMAIN_PREFIX: u8 = 0x01;

    /// Secure Merkle tree hasher with domain separation. This diverges from the
    /// original IETF spec. If you truly desire exact one-to-one behavior, enable
    /// the `unsafe-digest` feature and use `DigestMergeUnsafe`, which is not
    /// recommended for production usage.
    ///
    /// Uses domain separation prefixes to prevent [second preimage attacks]:
    /// - Leaves: `H(0x00 || data)`
    /// - Nodes: `H(0x01 || pos || left || right)`
    ///
    /// [second preimage attacks]: https://en.wikipedia.org/wiki/Merkle_tree#Second_preimage_attack
    pub struct DigestMerge<H: Digest>(PhantomData<H>);

    impl<H: Digest> Merge for DigestMerge<H> {
      type Item = digest::Output<H>;
      type Error = Infallible;

      fn leaf_hash(data: &[u8]) -> Result<Self::Item, Self::Error> {
        let mut hasher = H::new();
        hasher.update([LEAF_DOMAIN_PREFIX]);
        hasher.update(data);
        Ok(hasher.finalize())
      }

      fn merge_pos(
        pos: u64,
        left: &Self::Item,
        right: &Self::Item,
      ) -> Result<Self::Item, Self::Error> {
        let mut hasher = H::new();
        hasher.update([NODE_DOMAIN_PREFIX]);
        hasher.update(pos.to_be_bytes());
        hasher.update(left.as_slice());
        hasher.update(right.as_slice());
        Ok(hasher.finalize())
      }
    }
  }
}

cfg_if::cfg_if! {
  if #[cfg(feature = "unsafe-digest")] {
    /// Spec-compliant Merkle tree hasher **WITHOUT** domain separation.
    ///
    /// ## Safety
    /// Usage of this hasher is heavily discouraged, as it is vulnerable to 
    /// [second preimage attacks]. For production use, refer to [`DigestMerge`] 
    /// instead.
    ///
    /// [second preimage attacks]: https://en.wikipedia.org/wiki/Merkle_tree#Second_preimage_attack
    #[deprecated(
      note = "Vulnerable to second preimage attacks, use DigestMerge instead"
    )]
    pub struct DigestMergeUnsafe<H: digest::Digest>(core::marker::PhantomData<H>);

    #[allow(deprecated)]
    impl<H: digest::Digest> Merge for DigestMergeUnsafe<H> {
      type Item = digest::Output<H>;
      type Error = Infallible;

      fn leaf_hash(data: &[u8]) -> Result<Self::Item, Self::Error> {
        let mut hasher = H::new();
        hasher.update(data);
        Ok(hasher.finalize())
      }

      fn merge_pos(
        pos: u64,
        left: &Self::Item,
        right: &Self::Item,
      ) -> Result<Self::Item, Self::Error> {
        let mut hasher = H::new();
        hasher.update(pos.to_be_bytes());
        hasher.update(left.as_slice());
        hasher.update(right.as_slice());
        Ok(hasher.finalize())
      }
    }
  }
}
