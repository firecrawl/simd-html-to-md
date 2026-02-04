//! SIMD-accelerated byte scanning primitives.

use std::simd::{cmp::SimdPartialEq, Simd};

/// Number of lanes for SIMD operations.
const LANES: usize = 16;

/// SIMD vector type for byte operations.
type SimdVec = Simd<u8, LANES>;

/// Find the first occurrence of a byte in a slice using SIMD.
///
/// Returns `Some(index)` if found, `None` otherwise.
#[inline]
pub fn find_char(haystack: &[u8], needle: u8) -> Option<usize> {
    let needle_vec = SimdVec::splat(needle);
    let chunks = haystack.chunks_exact(LANES);
    let remainder = chunks.remainder();

    for (i, chunk) in chunks.enumerate() {
        let vec = SimdVec::from_slice(chunk);
        let mask = vec.simd_eq(needle_vec).to_bitmask();
        if mask != 0 {
            return Some(i * LANES + mask.trailing_zeros() as usize);
        }
    }

    // Scalar fallback for remainder
    let base = haystack.len() - remainder.len();
    for (i, &b) in remainder.iter().enumerate() {
        if b == needle {
            return Some(base + i);
        }
    }
    None
}

/// Find the first occurrence of any of the given bytes in a slice using SIMD.
///
/// Returns `Some((index, byte))` if found, `None` otherwise.
#[inline]
#[allow(dead_code)]
pub fn find_any(haystack: &[u8], needles: &[u8]) -> Option<(usize, u8)> {
    let index = find_any_index(haystack, needles)?;
    Some((index, haystack[index]))
}

/// Find the first occurrence of any of the given bytes in a slice using SIMD.
///
/// Returns `Some(index)` if found, `None` otherwise.
#[inline]
pub fn find_any_index(haystack: &[u8], needles: &[u8]) -> Option<usize> {
    if needles.is_empty() {
        return None;
    }

    let chunks = haystack.chunks_exact(LANES);
    let remainder = chunks.remainder();

    for (chunk_idx, chunk) in chunks.enumerate() {
        let vec = SimdVec::from_slice(chunk);
        let mut combined_mask: u64 = 0;

        for &needle in needles {
            let needle_vec = SimdVec::splat(needle);
            let mask = vec.simd_eq(needle_vec).to_bitmask() as u64;
            combined_mask |= mask;
        }

        if combined_mask != 0 {
            let first_pos = combined_mask.trailing_zeros() as usize;
            return Some(chunk_idx * LANES + first_pos);
        }
    }

    // Scalar fallback for remainder
    let base = haystack.len() - remainder.len();
    for (i, &b) in remainder.iter().enumerate() {
        for &needle in needles {
            if b == needle {
                return Some(base + i);
            }
        }
    }
    None
}

/// Find the first occurrence of '<' character.
#[inline]
pub fn find_lt(bytes: &[u8]) -> Option<usize> {
    find_char(bytes, b'<')
}

/// Find the first occurrence of '>' character.
#[inline]
pub fn find_gt(bytes: &[u8]) -> Option<usize> {
    find_char(bytes, b'>')
}

/// Find the first occurrence of '&' character.
#[inline]
pub fn find_amp(bytes: &[u8]) -> Option<usize> {
    find_char(bytes, b'&')
}

/// Find the first occurrence of '<', '>', or '&'.
#[inline]
#[allow(dead_code)]
pub fn find_special(bytes: &[u8]) -> Option<(usize, u8)> {
    find_any(bytes, &[b'<', b'>', b'&'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_char() {
        assert_eq!(find_char(b"hello world", b'w'), Some(6));
        assert_eq!(find_char(b"hello world", b'x'), None);
        assert_eq!(find_char(b"hello world", b'h'), Some(0));
        assert_eq!(find_char(b"hello world", b'd'), Some(10));
    }

    #[test]
    fn test_find_char_simd_boundary() {
        // Test at SIMD boundary (16 bytes)
        let data = b"0123456789abcdef<more";
        assert_eq!(find_char(data, b'<'), Some(16));

        // Test within first SIMD chunk
        let data = b"hello<world";
        assert_eq!(find_char(data, b'<'), Some(5));
    }

    #[test]
    fn test_find_any() {
        assert_eq!(find_any(b"hello <world>", &[b'<', b'>']), Some((6, b'<')));
        assert_eq!(find_any(b"hello world>", &[b'<', b'>']), Some((11, b'>')));
        assert_eq!(find_any(b"hello world", &[b'<', b'>']), None);
    }

    #[test]
    fn test_find_any_index() {
        assert_eq!(find_any_index(b"hello <world>", &[b'<', b'>']), Some(6));
        assert_eq!(find_any_index(b"hello world>", &[b'<', b'>']), Some(11));
        assert_eq!(find_any_index(b"hello world", &[b'<', b'>']), None);
    }

    #[test]
    fn test_find_lt() {
        assert_eq!(find_lt(b"<html>"), Some(0));
        assert_eq!(find_lt(b"text<tag>"), Some(4));
        assert_eq!(find_lt(b"no tags here"), None);
    }

    #[test]
    fn test_find_special() {
        assert_eq!(find_special(b"a&b<c>d"), Some((1, b'&')));
        assert_eq!(find_special(b"text<tag>"), Some((4, b'<')));
        assert_eq!(find_special(b"just text"), None);
    }

    #[test]
    fn test_large_input() {
        // Test with input larger than SIMD chunk
        let mut data = vec![b'x'; 100];
        data[50] = b'<';
        assert_eq!(find_lt(&data), Some(50));

        data[20] = b'<';
        assert_eq!(find_lt(&data), Some(20));
    }
}
