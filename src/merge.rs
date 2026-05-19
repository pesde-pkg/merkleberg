use core::{convert::Infallible, error::Error};

pub trait Merge {
  type Item: Clone + PartialEq;
  type Error: Error + Send + Sync + 'static;

  fn leaf_hash(data: &[u8]) -> Result<Self::Item, Self::Error>;

  fn merge_pos(
    pos: u64,
    left: &Self::Item,
    right: &Self::Item,
  ) -> Result<Self::Item, Self::Error>;

  fn merge(
    left: &Self::Item,
    right: &Self::Item,
  ) -> Result<Self::Item, Self::Error> {
    Self::merge_pos(0, left, right)
  }

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
    /// original IETF spec. If you truly desire exact one-to-one behavior, refer to
    /// [`DigestMergeUnsafe`], which is not recommended for production usage.
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
    /// Usage of this hasher is heavily recommended against, as it is
    /// vulnerable to [second preimage attacks]. For production use,
    /// refer to [`DigestMerge`] instead.
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
