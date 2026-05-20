/// Alias for user-provided errors.
///
/// Store and merge errors are boxed to enable error propagation
/// across different error types.
pub type UserError = Box<dyn core::error::Error + Send + Sync + 'static>;

/// Result type for MMR operations.
pub type Result<T> = core::result::Result<T, Error>;

/// Error type for MMR operations.
///
/// Errors can originate from:
/// - Store operations (IO failures, missing elements)
/// - Merge operations (hash computation failures)
/// - Invalid API usage (operations on empty MMR)
/// - Proof verification failures
/// 
/// Store and merge errors are provided by the implementer of the trait
/// and can vary. See the [`UserError`] type. 
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// Attempted to get root of an empty MMR.
  #[error("Get root on an empty MMR")]
  GetRootOnEmpty,

  /// Store returned an element that doesn't exist.
  ///
  /// This indicates the MMR size is out of sync with the store,
  /// or elements were deleted externally.
  #[error("Inconsistent store")]
  InconsistentStore,

  /// Proof verification failed.
  ///
  /// The proof structure is invalid or corrupted.
  #[error("Corrupted proof")]
  CorruptedProof,

  /// Attempted to generate proof for a non-leaf node.
  ///
  /// Only leaf nodes can have inclusion proofs in standard MMR.
  #[error("Tried to verify membership of a non-leaf")]
  NodeProofsNotSupported,

  /// Invalid leaf indices for proof generation.
  #[error("Generate proof for invalid leaves")]
  GenProofForInvalidLeaves,

  /// Error from the storage backend.
  #[error("Store error: {0}")]
  StoreError(UserError),

  /// Error from merge operations.
  #[error("Merge error: {0}")]
  MergeError(UserError),
}

impl PartialEq for Error {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Error::StoreError(_), Error::StoreError(_)) => false,
      (Error::MergeError(_), Error::MergeError(_)) => false,
      _ => std::mem::discriminant(self) == std::mem::discriminant(other),
    }
  }
}
