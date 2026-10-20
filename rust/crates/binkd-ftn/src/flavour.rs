//! The flavour letters of a queue entry, the port of the `prio` string of
//! `ftnq.c` and of the `MAXFLVR()` macro of `ftnq.h`.

/// The flavour priority, the best first.
pub const PRIO: &[u8] = b"IiCcDdOoFfHh";

/// Returns the index of a flavour in the priority string.
///
/// The value 0 is the end of the string in the C code, so it is worse than
/// every flavour. A value that is not a flavour gives the null pointer of
/// `strchr()`, which is smaller than every pointer into the string.
pub fn index(flvr: i32) -> usize {
    if flvr == 0 {
        return PRIO.len();
    }
    PRIO.iter().position(|&c| c as i32 == flvr).unwrap_or(0)
}

/// Returns the better of two flavours.
pub fn max(a: i32, b: i32) -> i32 {
    if index(a) < index(b) {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order() {
        assert_eq!(max(b'I' as i32, b'f' as i32), b'I' as i32);
        assert_eq!(max(b'f' as i32, 0), b'f' as i32);
        assert_eq!(max(b'h' as i32, b'H' as i32), b'H' as i32);
    }
}
