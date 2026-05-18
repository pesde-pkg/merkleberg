cfg_if::cfg_if! {
  if #[cfg(feature = "std")] {
    use std::result::Result as InnerResult;
    use std::error::Error as InnerError;
  } else {
    use core::result::Result as InnerResult;
    use core::error::Error as InnerError;
  }
}

pub type Result<T, E = Box<dyn InnerError + Send + 'static>> =
  InnerResult<T, Error<E>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error<E = Box<dyn InnerError + Send + 'static>> {
  GetRootOnEmpty,
  InconsistentStore,
  StoreError(E),
  CorruptedProof,
  NodeProofsNotSupported,
  GenProofForInvalidLeaves,
  MergeError(crate::string::String),
}

impl<E: core::fmt::Display> core::fmt::Display for Error<E> {
  fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
    match self {
      Error::GetRootOnEmpty => write!(f, "Get root on an empty MMR"),
      Error::InconsistentStore => write!(f, "Inconsistent store"),
      Error::StoreError(e) => write!(f, "Store error: {}", e),
      Error::CorruptedProof => write!(f, "Corrupted proof"),
      Error::NodeProofsNotSupported => {
        write!(f, "Tried to verify membership of a non-leaf")
      }
      Error::GenProofForInvalidLeaves => {
        write!(f, "Generate proof for invalid leaves")
      }
      Error::MergeError(msg) => write!(f, "Merge error: {}", msg),
    }
  }
}

impl<E: InnerError + core::fmt::Display> InnerError for Error<E> {}
