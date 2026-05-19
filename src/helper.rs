use crate::vec;
use crate::vec::Vec;

pub fn leaf_index_to_pos(index: u64) -> u64 {
  // mmr_size - H - 1, H is the height(intervals) of last peak
  leaf_index_to_mmr_size(index) - (index + 1).trailing_zeros() as u64 - 1
}

pub fn leaf_index_to_mmr_size(index: u64) -> u64 {
  // leaf index start with 0
  let leaves_count = index + 1;

  // the peak count(k) is actually the count of 1 in leaves count's binary representation
  let peak_count = leaves_count.count_ones() as u64;

  2 * leaves_count - peak_count
}

pub fn pos_height_in_tree(mut pos: u64) -> u8 {
  if pos == 0 {
    return 0;
  }

  let mut peak_size = u64::MAX >> pos.leading_zeros();
  while peak_size > 0 {
    if pos >= peak_size {
      pos -= peak_size;
    }
    peak_size >>= 1;
  }
  pos as u8
}

pub fn parent_offset(height: u8) -> u64 {
  2 << height
}

pub fn sibling_offset(height: u8) -> u64 {
  (2 << height) - 1
}

/// Returns the height of the peaks in the mmr, presented by a bitmap.
/// for example, for a mmr with 11 leaves, the mmr_size is 19, it will return 0b1011.
/// 0b1011 indicates that the left peaks are at height 0, 1 and 3.
///           14
///        /       \
///      6          13
///    /   \       /   \
///   2     5     9     12     17
///  / \   /  \  / \   /  \   /  \
/// 0   1 3   4 7   8 10  11 15  16 18
///
/// please note that when the mmr_size is invalid, it will return the bitmap of the last valid mmr.
/// in the below example, the mmr_size is 6, but it's not a valid mmr, it will return 0b11.
///   2     5
///  / \   /  \
/// 0   1 3   4
pub fn get_peak_map(mmr_size: u64) -> u64 {
  if mmr_size == 0 {
    return 0;
  }

  let mut pos = mmr_size;
  let mut peak_size = u64::MAX >> pos.leading_zeros();
  let mut peak_map = 0;
  while peak_size > 0 {
    peak_map <<= 1;
    if pos >= peak_size {
      pos -= peak_size;
      peak_map |= 1;
    }
    peak_size >>= 1;
  }

  peak_map
}

/// Returns the pos of the peaks in the mmr.
/// for example, for a mmr with 11 leaves, the mmr_size is 19, it will return [14, 17, 18].
///           14
///        /       \
///      6          13
///    /   \       /   \
///   2     5     9     12     17
///  / \   /  \  / \   /  \   /  \
/// 0   1 3   4 7   8 10  11 15  16 18
///
/// please note that when the mmr_size is invalid, it will return the peaks of the last valid mmr.
/// in the below example, the mmr_size is 6, but it's not a valid mmr, it will return [2, 3].
///   2     5
///  / \   /  \
/// 0   1 3   4
pub fn get_peaks(mmr_size: u64) -> Vec<u64> {
  if mmr_size == 0 {
    return vec![];
  }

  let leading_zeros = mmr_size.leading_zeros();
  let mut pos = mmr_size;
  let mut peak_size = u64::MAX >> leading_zeros;
  let mut peaks = Vec::with_capacity(64 - leading_zeros as usize);
  let mut peaks_sum = 0;
  while peak_size > 0 {
    if pos >= peak_size {
      pos -= peak_size;
      peaks.push(peaks_sum + peak_size - 1);
      peaks_sum += peak_size;
    }
    peak_size >>= 1;
  }
  peaks
}

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

fn log2floor(x: u64) -> u32 {
  if x == 0 {
    return 0;
  }
  u64::BITS - x.leading_zeros() - 1
}

pub fn index_height_mmriver(i: u64) -> u8 {
  let mut pos = i + 1;
  while !all_ones(pos) {
    pos = pos - (most_sig_bit(pos) - 1);
  }
  u64::BITS as u8 - pos.leading_zeros() as u8 - 1
}

pub fn peaks_mmriver(i: u64) -> Vec<u64> {
  let mut peak = 0;
  let mut peaks = vec![];
  let mut s = i + 1;
  while s != 0 {
    let highest_size = (1 << log2floor(s + 1)) - 1;
    peak += highest_size;
    peaks.push(peak - 1);
    s -= highest_size;
  }
  peaks
}

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

pub fn consistency_proof_paths(ifrom: u64, ito: u64) -> Vec<Vec<u64>> {
  peaks_mmriver(ifrom)
    .into_iter()
    .map(|ipeak| inclusion_proof_path(ipeak, ito))
    .collect()
}
