//! Helper functions for MMR position calculations.
//!
//! Provides utilities for converting indices, computing peaks, and tree heights.
//! Most users don't need this module directly; it's primarily used internally
//! by [`crate::MMR`] and [`crate::MMRIVER`].
//!
//! ## References
//!
//! - [OpenTimestamps MMR spec](https://github.com/opentimestamps/opentimestamps-server/blob/master/doc/merkle-mountain-range.md)
//! - [Grin documentation](https://docs.grin.mw/wiki/chain-state/merkle-mountain-range/)

use crate::vec;
use crate::vec::Vec;

/// Convert leaf index to MMR position.
#[must_use]
pub fn leaf_index_to_pos(index: u64) -> u64 {
  // mmr_size - H - 1, H is the height(intervals) of last peak
  leaf_index_to_mmr_size(index) - (index + 1).trailing_zeros() as u64 - 1
}

/// Convert leaf index to MMR size.
///
/// Returns total positions (leaves + nodes) for a given leaf count.
#[must_use]
pub fn leaf_index_to_mmr_size(index: u64) -> u64 {
  // leaf index start with 0
  let leaves_count = index + 1;

  // the peak count(k) is actually the count of 1 in leaves count's binary representation
  let peak_count = leaves_count.count_ones() as u64;

  2 * leaves_count - peak_count
}

/// Compute height of position in tree.
#[must_use]
pub fn pos_height_in_tree(mut pos: u64) -> u8 {
  if pos == 0 {
    return 0;
  }

  let mut peak_size = u64::MAX >> pos.leading_zeros();
  while peak_size > 0 {
    if pos >= peak_size {
      pos -= peak_size;
    }
    peak_size >>= 1u64;
  }
  pos as u8
}

/// Offset to parent from node at given height.
#[must_use]
pub fn parent_offset(height: u8) -> u64 {
  2 << height
}

/// Offset to sibling from node at given height.
#[must_use]
pub fn sibling_offset(height: u8) -> u64 {
  (2 << height) - 1
}

/// Returns a bitmap representing the heights of the peaks in the MMR.
///
/// ## Bitmap Format
///
/// Each set bit at position `n` indicates a peak at height `n`. For example,
/// `0b1011` means peaks exist at heights 0, 1, and 3.
///
/// ## Example
///
/// An MMR with 11 leaves has an `mmr_size` of 19 and returns `0b1011`:
///
/// ```text
///           14
///        /       \
///      6          13
///    /   \       /   \
///   2     5     9     12     17
///  / \   /  \  / \   /  \   /  \
/// 0   1 3   4 7   8 10  11 15  16 18
/// ```
///
/// ## Invalid MMR Sizes
///
/// If `mmr_size` does not correspond to a valid MMR, the bitmap of the last
/// valid MMR is returned instead. For example, `mmr_size = 6` is invalid and
/// returns `0b11`, equivalent to:
///
/// ```text
///   2     5
///  / \   /  \
/// 0   1 3   4
/// ```
#[must_use]
pub fn get_peak_map(mmr_size: u64) -> u64 {
  if mmr_size == 0 {
    return 0;
  }

  let mut pos = mmr_size;
  let mut peak_size = u64::MAX >> pos.leading_zeros();
  let mut peak_map = 0;
  while peak_size > 0 {
    peak_map <<= 1u64;
    if pos >= peak_size {
      pos -= peak_size;
      peak_map |= 1;
    }
    peak_size >>= 1u64;
  }

  peak_map
}

/// Iterator over peak positions in MMR.
#[must_use]
pub struct PeaksIter {
  pos: u64,
  peak_size: u64,
  peaks_sum: u64,
}

impl PeaksIter {
  /// Create iterator for peaks in MMR of given size.
  pub fn new(mmr_size: u64) -> Self {
    if mmr_size == 0 {
      return Self {
        pos: 0,
        peak_size: 0,
        peaks_sum: 0,
      };
    }
    Self {
      pos: mmr_size,
      peak_size: u64::MAX >> mmr_size.leading_zeros(),
      peaks_sum: 0,
    }
  }
}

impl Iterator for PeaksIter {
  type Item = u64;

  fn next(&mut self) -> Option<Self::Item> {
    let mut peak_opt = None;

    while self.peak_size > 0 && peak_opt.is_none() {
      if self.pos >= self.peak_size {
        self.pos -= self.peak_size;
        let peak = self.peaks_sum + self.peak_size - 1;
        self.peaks_sum += self.peak_size;
        peak_opt = Some(peak);
      }
      self.peak_size >>= 1u64;
    }

    peak_opt
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    if self.peak_size == 0 {
      return (0, Some(0));
    }
    #[allow(clippy::integer_division)]
    let remaining_peaks = (self.pos / self.peak_size + 1).count_ones() as usize;
    (remaining_peaks, Some(remaining_peaks))
  }
}

impl ExactSizeIterator for PeaksIter {}

fn all_ones(pos: u64) -> bool {
  if pos == 0 {
    return false;
  }
  let imsb = u64::BITS - pos.leading_zeros() - 1;
  let mask = (1 << (imsb + 1)) - 1;
  pos == mask
}

fn most_sig_bit(pos: u64) -> u64 {
  if pos == 0 {
    return 0;
  }
  1 << (u64::BITS - pos.leading_zeros() - 1)
}

/// Compute height at index for MMRIVER.
#[must_use]
pub fn index_height_mmriver(i: u64) -> u8 {
  let mut pos = i + 1;
  while !all_ones(pos) {
    pos = pos - (most_sig_bit(pos) - 1);
  }
  u64::BITS as u8 - pos.leading_zeros() as u8 - 1
}

/// Iterator over peak positions in MMRIVER.
///
/// Uses MMRIVER-specific indexing scheme.
#[derive(Clone)]
#[must_use]
pub struct PeaksMMRIVERIter {
  peak: u64,
  remaining: u64,
}

impl PeaksMMRIVERIter {
  /// Create iterator for peaks at given index.
  pub fn new(i: u64) -> Self {
    Self {
      peak: 0,
      remaining: i + 1,
    }
  }
}

impl Iterator for PeaksMMRIVERIter {
  type Item = u64;

  fn next(&mut self) -> Option<Self::Item> {
    if self.remaining == 0 {
      return None;
    }
    let highest_size = (1 << u64::ilog2(self.remaining + 1)) - 1;
    self.peak += highest_size;
    self.remaining -= highest_size;
    Some(self.peak - 1)
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    let remaining_peaks = self.remaining.count_ones() as usize;
    (remaining_peaks, Some(remaining_peaks))
  }
}

impl ExactSizeIterator for PeaksMMRIVERIter {}

/// Compute inclusion proof path for MMRIVER.
#[must_use]
pub fn inclusion_proof_path(mut i: u64, c: u64) -> Vec<u64> {
  let mut path = vec![];
  let mut g = index_height_mmriver(i);

  loop {
    let sibling_offset = 2 << g;

    if index_height_mmriver(i + 1) > g {
      let isibling = i - sibling_offset + 1;
      i += 1;
      if isibling > c {
        return path;
      }
      path.push(isibling);
    } else {
      let isibling = i + sibling_offset - 1;
      i += sibling_offset;
      if isibling > c {
        return path;
      }
      path.push(isibling);
    }
    g += 1;
  }
}
