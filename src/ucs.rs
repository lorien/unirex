//! Borrowed views over CPython's three PEP 393 storage widths.

use std::ops::Range;

use memchr::memchr;

/// A UCS code unit that can be searched by a first-unit prefilter.
pub(crate) trait Unit: Copy + Into<u32> {
    /// The storage kind this unit represents.
    const KIND: UcsKind;

    /// Find the next index at or after `start` whose unit equals `target`.
    fn find_next_unit(hay: &[Self], target: u32, start: usize) -> Option<usize>;
}

impl Unit for u8 {
    const KIND: UcsKind = UcsKind::Ucs1;

    fn find_next_unit(hay: &[u8], target: u32, start: usize) -> Option<usize> {
        let byte = u8::try_from(target).ok()?;
        memchr(byte, &hay[start..]).map(|offset| start + offset)
    }
}

impl Unit for u16 {
    const KIND: UcsKind = UcsKind::Ucs2;

    fn find_next_unit(hay: &[u16], target: u32, start: usize) -> Option<usize> {
        if target > 0xFFFF {
            return None;
        }
        if target > 0xFF {
            return hay[start..]
                .iter()
                .position(|&unit| u32::from(unit) == target)
                .map(|index| start + index);
        }
        let byte = target as u8;
        let bytes: &[u8] = bytemuck::cast_slice(hay);
        let mut offset = start * std::mem::size_of::<u16>();
        while offset < bytes.len() {
            let relative = memchr(byte, &bytes[offset..])?;
            let position = offset + relative;
            let index = position / std::mem::size_of::<u16>();
            if index >= hay.len() {
                return None;
            }
            if u32::from(hay[index]) == target {
                return Some(index);
            }
            offset = position + 1;
        }
        None
    }
}

impl Unit for u32 {
    const KIND: UcsKind = UcsKind::Ucs4;

    fn find_next_unit(hay: &[u32], target: u32, start: usize) -> Option<usize> {
        if target > 0xFF {
            return hay[start..]
                .iter()
                .position(|&unit| unit == target)
                .map(|index| start + index);
        }
        let byte = target as u8;
        let bytes: &[u8] = bytemuck::cast_slice(hay);
        let mut offset = start * std::mem::size_of::<u32>();
        while offset < bytes.len() {
            let relative = memchr(byte, &bytes[offset..])?;
            let position = offset + relative;
            let index = position / std::mem::size_of::<u32>();
            if index >= hay.len() {
                return None;
            }
            if hay[index] == target {
                return Some(index);
            }
            offset = position + 1;
        }
        None
    }
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
}
