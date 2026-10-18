//! Borrowed views over CPython's three PEP 393 storage widths.

use std::ops::Range;

use memchr::memchr;

/// A packed pair finder over raw unit bytes.
///
/// The AVX2 implementation is used when the CPU supports it. It owns no
/// borrow, so a compiled pattern can keep one finder per UCS width. Callers
/// must verify the full unit after a candidate hit; this is only a prefilter.
#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ByteFinder {
    finder: memchr::arch::x86_64::avx2::packedpair::Finder,
}

#[cfg(target_arch = "x86_64")]
impl ByteFinder {
    pub(crate) fn new(needle: &[u8]) -> Option<ByteFinder> {
        memchr::arch::x86_64::avx2::packedpair::Finder::new(needle)
            .map(|finder| ByteFinder { finder })
    }

    pub(crate) fn min_haystack_len(&self) -> usize {
        self.finder.min_haystack_len()
    }

    pub(crate) fn find_prefilter(&self, hay: &[u8]) -> Option<usize> {
        self.finder.find_prefilter(hay)
    }
}

#[cfg(not(target_arch = "x86_64"))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ByteFinder;

#[cfg(not(target_arch = "x86_64"))]
impl ByteFinder {
    pub(crate) fn new(_needle: &[u8]) -> Option<ByteFinder> {
        None
    }
}

/// A prepared first-unit scan for one UCS width.
///
/// The scan is built once per search and reused across candidates. For UCS-2
/// and UCS-4 it searches the raw unit bytes for the whole native-endian unit,
/// which avoids scanning one byte at a time and avoids most unaligned hits.
pub(crate) enum Scan<'a> {
    /// A single code unit searched with `memchr` on UCS-1 or a scalar scan on
    /// the wider kinds when no byte finder is available.
    One(u32),
    /// A native-endian unit needle searched with a packed pair prefilter.
    Bytes { target: u32, finder: &'a ByteFinder },
    /// A small range set searched with a scalar membership test.
    Ranges(&'a [(u32, u32)]),
}

impl Scan<'_> {
    /// Find the next index at or after `start` that can begin a match.
    pub(crate) fn find_next<U: Unit>(&self, hay: &[U], start: usize) -> Option<usize> {
        U::find_next_with(self, hay, start)
    }
}

/// A UCS code unit that can be searched by a first-unit prefilter.
pub(crate) trait Unit: Copy + Into<u32> {
    /// The storage kind this unit represents.
    const KIND: UcsKind;

    /// Find the next index at or after `start` that can begin a match.
    fn find_next_with(scan: &Scan<'_>, hay: &[Self], start: usize) -> Option<usize>;
}

impl Unit for u8 {
    const KIND: UcsKind = UcsKind::Ucs1;

    fn find_next_with(scan: &Scan<'_>, hay: &[u8], start: usize) -> Option<usize> {
        if start > hay.len() {
            return None;
        }
        match scan {
            Scan::One(target) | Scan::Bytes { target, .. } => {
                let byte = u8::try_from(*target).ok()?;
                memchr(byte, &hay[start..]).map(|offset| start + offset)
            }
            Scan::Ranges(ranges) => find_in_ranges(hay, ranges, start),
        }
    }
}

impl Unit for u16 {
    const KIND: UcsKind = UcsKind::Ucs2;

    fn find_next_with(scan: &Scan<'_>, hay: &[u16], start: usize) -> Option<usize> {
        if start > hay.len() {
            return None;
        }
        match scan {
            Scan::One(target) => find_unit(hay, *target, start),
            Scan::Bytes { target, finder } => find_in_bytes(hay, *target, finder, start),
            Scan::Ranges(ranges) => find_in_ranges(hay, ranges, start),
        }
    }
}

impl Unit for u32 {
    const KIND: UcsKind = UcsKind::Ucs4;

    fn find_next_with(scan: &Scan<'_>, hay: &[u32], start: usize) -> Option<usize> {
        if start > hay.len() {
            return None;
        }
        match scan {
            Scan::One(target) => find_unit(hay, *target, start),
            Scan::Bytes { target, finder } => find_in_bytes(hay, *target, finder, start),
            Scan::Ranges(ranges) => find_in_ranges(hay, ranges, start),
        }
    }
}

fn find_unit<U: Unit>(hay: &[U], target: u32, start: usize) -> Option<usize> {
    hay[start..]
        .iter()
        .position(|unit| (*unit).into() == target)
        .map(|index| start + index)
}

fn find_in_bytes<U: Unit + bytemuck::Pod>(
    hay: &[U],
    target: u32,
    finder: &ByteFinder,
    start: usize,
) -> Option<usize> {
    let width = std::mem::size_of::<U>();
    let bytes: &[u8] = bytemuck::cast_slice(hay);
    let mut offset = start.checked_mul(width)?;
    let min_len = finder.min_haystack_len();
    while offset < bytes.len() {
        if bytes.len() - offset < min_len {
            break;
        }
        let relative = finder.find_prefilter(&bytes[offset..])?;
        let position = offset + relative;
        let index = position / width;
        if position % width == 0 && hay.get(index).is_some_and(|unit| (*unit).into() == target) {
            return Some(index);
        }
        offset = position + 1;
    }
    // The vector prefilter requires a minimum slice length. Search the short
    // tail, and the whole haystack when it is below that minimum, one unit at
    // a time. Flooring the byte offset may recheck one unit, which is safe.
    find_unit(hay, target, offset / width)
}

fn find_in_ranges<U: Unit>(hay: &[U], ranges: &[(u32, u32)], start: usize) -> Option<usize> {
    hay[start..]
        .iter()
        .position(|unit| {
            let code_point = (*unit).into();
            ranges
                .iter()
                .any(|&(lo, hi)| code_point >= lo && code_point <= hi)
        })
        .map(|index| start + index)
}

/// The storage width of a UCS haystack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UcsKind {
    /// One byte per code unit (`PyUnicode_1BYTE_KIND`).
    Ucs1,
    /// Two bytes per code unit (`PyUnicode_2BYTE_KIND`).
    Ucs2,
    /// Four bytes per code unit (`PyUnicode_4BYTE_KIND`).
    Ucs4,
}

/// A borrowed UCS haystack.
///
/// The core engine never converts these units to UTF-8. Each unit is treated
/// as one code point. In particular, UCS-2 lone surrogates are ordinary code
/// points and are not paired.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ucs<'a> {
    /// UCS-1 storage.
    Ucs1(&'a [u8]),
    /// UCS-2 storage.
    Ucs2(&'a [u16]),
    /// UCS-4 storage.
    Ucs4(&'a [u32]),
}

impl<'a> Ucs<'a> {
    /// The number of code units in the haystack.
    pub fn len(self) -> usize {
        match self {
            Ucs::Ucs1(units) => units.len(),
            Ucs::Ucs2(units) => units.len(),
            Ucs::Ucs4(units) => units.len(),
        }
    }

    /// Whether the haystack is empty.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// The storage width.
    pub fn kind(self) -> UcsKind {
        match self {
            Ucs::Ucs1(_) => UcsKind::Ucs1,
            Ucs::Ucs2(_) => UcsKind::Ucs2,
            Ucs::Ucs4(_) => UcsKind::Ucs4,
        }
    }

    /// The code point at `index`, widened to `u32`.
    pub fn get(self, index: usize) -> Option<u32> {
        match self {
            Ucs::Ucs1(units) => units.get(index).map(|&u| u32::from(u)),
            Ucs::Ucs2(units) => units.get(index).map(|&u| u32::from(u)),
            Ucs::Ucs4(units) => units.get(index).copied(),
        }
    }

    /// Slice by code-unit index, preserving the storage width.
    pub fn slice(self, range: Range<usize>) -> Option<Ucs<'a>> {
        if range.start > range.end || range.end > self.len() {
            return None;
        }
        match self {
            Ucs::Ucs1(units) => units.get(range).map(Ucs::Ucs1),
            Ucs::Ucs2(units) => units.get(range).map(Ucs::Ucs2),
            Ucs::Ucs4(units) => units.get(range).map(Ucs::Ucs4),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_access() {
        let one = *b"ab";
        let two = [0x03B1u16, 0x03B2];
        let four = [0x1F600u32, 0x1F601];
        assert_eq!(Ucs::Ucs1(&one).kind(), UcsKind::Ucs1);
        assert_eq!(Ucs::Ucs2(&two).kind(), UcsKind::Ucs2);
        assert_eq!(Ucs::Ucs4(&four).kind(), UcsKind::Ucs4);
        assert_eq!(Ucs::Ucs1(&one).get(1), Some(u32::from(b'b')));
        assert_eq!(Ucs::Ucs2(&two).get(0), Some(0x03B1));
        assert_eq!(Ucs::Ucs4(&four).get(0), Some(0x1F600));
        assert_eq!(Ucs::Ucs4(&four).get(2), None);
    }

    #[test]
    fn slicing_preserves_kind() {
        let units = [1u16, 2, 3, 4];
        let ucs = Ucs::Ucs2(&units);
        assert_eq!(ucs.slice(1..3), Some(Ucs::Ucs2(&units[1..3])));
        let reversed = (3, 1);
        assert_eq!(ucs.slice(reversed.0..reversed.1), None);
        assert_eq!(ucs.slice(0..5), None);
    }

    #[test]
    fn lone_surrogate_is_a_unit() {
        let units = [0xD800u16, 0x0041];
        let ucs = Ucs::Ucs2(&units);
        assert_eq!(ucs.len(), 2);
        assert_eq!(ucs.get(0), Some(0xD800));
        assert_eq!(ucs.get(1), Some(0x0041));
    }

    #[test]
    fn scan_one_and_ranges() {
        let one = Scan::One(u32::from(b'b'));
        assert_eq!(one.find_next(b"abc", 0), Some(1));
        assert_eq!(one.find_next(b"abc", 2), None);
        assert_eq!(one.find_next(b"abc", 4), None);

        let too_wide = Scan::One(0x03B1);
        assert_eq!(too_wide.find_next(b"abc", 0), None);

        let ranges = [(0x61, 0x63), (0x30, 0x39)];
        let scan = Scan::Ranges(&ranges);
        assert_eq!(scan.find_next(b"..b1", 0), Some(2));
        assert_eq!(scan.find_next(b"..b1", 3), Some(3));
        assert_eq!(scan.find_next(b"....", 0), None);
        assert_eq!(scan.find_next(b"....", 5), None);
    }

    #[test]
    fn scan_bytes_finds_aligned_units() {
        let target = 0x03B1u16;
        let needle = target.to_ne_bytes();
        let hay = [0x0041u16, 0x03B1, 0x03B1, 0x0042];
        let Some(finder) = ByteFinder::new(&needle) else {
            return;
        };
        let scan = Scan::Bytes {
            target: u32::from(target),
            finder: &finder,
        };
        assert_eq!(scan.find_next(&hay, 0), Some(1));
        assert_eq!(scan.find_next(&hay, 2), Some(2));
        assert_eq!(scan.find_next(&hay, 3), None);
        assert_eq!(scan.find_next(&hay, 5), None);
    }

    #[test]
    fn scan_bytes_skips_unaligned_hits_u16() {
        let target = 0x0040u16;
        let needle = target.to_ne_bytes();
        let mut units = [0u16; 8];
        let bytes: &mut [u8] = bytemuck::cast_slice_mut(&mut units);
        bytes[1..3].copy_from_slice(&needle);
        bytes[10..12].copy_from_slice(&needle);
        let Some(finder) = ByteFinder::new(&needle) else {
            return;
        };
        let scan = Scan::Bytes {
            target: u32::from(target),
            finder: &finder,
        };
        assert_eq!(scan.find_next(&units, 0), Some(5));
        assert_eq!(scan.find_next(&units, 6), None);
        assert_eq!(scan.find_next(&units, 8), None);
        assert_eq!(scan.find_next(&units, 9), None);
    }

    #[test]
    fn scan_bytes_searches_short_tail() {
        let target = 0x0040u16;
        let needle = target.to_ne_bytes();
        let Some(finder) = ByteFinder::new(&needle) else {
            return;
        };
        let scan = Scan::Bytes {
            target: u32::from(target),
            finder: &finder,
        };
        let mut short = [0u16; 3];
        short[2] = target;
        assert_eq!(scan.find_next(&short, 0), Some(2));
        let mut long = [0u16; 20];
        long[19] = target;
        assert_eq!(scan.find_next(&long, 0), Some(19));
        assert_eq!(scan.find_next(&long, 19), Some(19));
        assert_eq!(scan.find_next(&long, 20), None);
    }

    #[test]
    fn scan_bytes_skips_unaligned_hits_u32() {
        let target = 0x1F600u32;
        let needle = target.to_ne_bytes();
        let mut units = [0u32; 8];
        let bytes: &mut [u8] = bytemuck::cast_slice_mut(&mut units);
        bytes[1..5].copy_from_slice(&needle);
        bytes[20..24].copy_from_slice(&needle);
        let Some(finder) = ByteFinder::new(&needle) else {
            return;
        };
        let scan = Scan::Bytes {
            target,
            finder: &finder,
        };
        assert_eq!(scan.find_next(&units, 0), Some(5));
        assert_eq!(scan.find_next(&units, 6), None);
        assert_eq!(scan.find_next(&units, 8), None);
        assert_eq!(scan.find_next(&units, 9), None);

        let mut tail = [0u32; 12];
        tail[11] = target;
        assert_eq!(scan.find_next(&tail, 0), Some(11));
        assert_eq!(scan.find_next(&tail, 11), Some(11));
        assert_eq!(scan.find_next(&tail, 12), None);
    }
}
