pub type UserError = Box<dyn core::error::Error + Send + Sync + 'static>;
pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Get root on an empty MMR")]
  GetRootOnEmpty,
  #[error("Inconsistent store")]
  InconsistentStore,
  #[error("Corrupted proof")]
  CorruptedProof,
  #[error("Tried to verify membership of a non-leaf")]
  NodeProofsNotSupported,
  #[error("Generate proof for invalid leaves")]
  GenProofForInvalidLeaves,
  #[error("Store error: {0}")]
  StoreError(UserError),
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
