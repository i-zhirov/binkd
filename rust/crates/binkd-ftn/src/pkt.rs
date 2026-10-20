//! Packet header handling (FTS-1 and type-2), the port of `pkt_getaddr()`
//! and `pkt_setaddr()` of `tools.c`.

/// The packet address fields: `oz onet onode op dz dnet dnode dp`.
pub type PktAddr = [i16; 8];

fn get(raw: &[u8], x: usize) -> u8 {
    raw.get(x).copied().unwrap_or(0)
}

fn get_byte2(raw: &[u8], x: usize) -> i32 {
    get(raw, x) as i32 + get(raw, x + 1) as i32 * 0x100
}

fn set_byte2(raw: &mut [u8], x: usize, v: i16) {
    let v = v as i32;
    raw[x] = (v & 0xff) as u8;
    raw[x + 1] = ((v / 0x100) & 0xff) as u8;
}

/// Returns the addresses of a packet header.
///
/// A field is -1 when the packet does not hold it. `None` means a packet
/// of another version.
pub fn pkt_getaddr(raw: &[u8]) -> Option<PktAddr> {
    let mut v: PktAddr = [-1; 8];

    if get_byte2(raw, 18) != 2 {
        return None;
    }

    // The FTS-1 net and node fields.
    v[2] = get_byte2(raw, 0) as i16;
    v[1] = get_byte2(raw, 20) as i16;
    v[6] = get_byte2(raw, 2) as i16;
    v[5] = get_byte2(raw, 22) as i16;

    // qmail and zmail are not point aware: the zone is at other offsets.
    if get(raw, 24) == 0x29 || get(raw, 24) == 0x35 {
        v[0] = get_byte2(raw, 34) as i16;
        v[4] = get_byte2(raw, 36) as i16;
    } else if get(raw, 44) & 1 != 0 {
        // Type-2 packets are point aware.
        v[0] = get_byte2(raw, 46) as i16;
        v[3] = get_byte2(raw, 50) as i16;
        v[4] = get_byte2(raw, 48) as i16;
        v[7] = get_byte2(raw, 52) as i16;

        // Type-2+ packets move the net field for the point.
        if get(raw, 44) == get(raw, 41)
            && get(raw, 45) == get(raw, 40)
            && get_byte2(raw, 50) != 0
            && v[1] == -1
        {
            v[1] = get_byte2(raw, 38) as i16;
        }
    }

    Some(v)
}

/// Sets the addresses of a packet header.
///
/// A field with the value -1 stays unchanged. Returns false for a packet
/// of another version.
pub fn pkt_setaddr(raw: &mut [u8], v: &PktAddr) -> bool {
    if get_byte2(raw, 18) != 2 {
        return false;
    }

    // The FTS-1 net and node fields.
    if v[2] >= 0 {
        set_byte2(raw, 0, v[2]);
    }
    if v[1] >= 0 {
        set_byte2(raw, 20, v[1]);
    }
    if v[6] >= 0 {
        set_byte2(raw, 2, v[6]);
    }
    if v[5] >= 0 {
        set_byte2(raw, 22, v[5]);
    }

    // qmail and zmail are not point aware: the zone is at other offsets.
    if get(raw, 24) == 0x29 || get(raw, 24) == 0x35 {
        if v[0] >= 0 {
            set_byte2(raw, 34, v[0]);
        }
        if v[4] >= 0 {
            set_byte2(raw, 36, v[4]);
        }
    } else if get(raw, 44) & 1 != 0 {
        // Type-2 packets are point aware.
        if v[0] >= 0 {
            set_byte2(raw, 46, v[0]);
        }
        if v[3] >= 0 {
            set_byte2(raw, 50, v[3]);
        }
        if v[4] >= 0 {
            set_byte2(raw, 48, v[4]);
        }
        if v[7] >= 0 {
            set_byte2(raw, 52, v[7]);
        }

        // Type-2+ packets move the net field for the point.
        if get(raw, 44) == get(raw, 41) && get(raw, 45) == get(raw, 40) && get_byte2(raw, 50) != 0 {
            raw[38] = raw[20];
            raw[39] = raw[21];
            raw[20] = 0xff;
            raw[21] = 0xff;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut raw = [0u8; 64];
        raw[18] = 2;
        assert!(pkt_setaddr(&mut raw, &[2, 5047, 13, -1, -1, -1, -1, -1]));
        let v = pkt_getaddr(&raw).unwrap();
        assert_eq!(v[0], -1);
        assert_eq!(v[1], 5047);
        assert_eq!(v[2], 13);
    }

    #[test]
    fn wrong_version() {
        let mut raw = [0u8; 64];
        raw[18] = 1;
        assert!(!pkt_setaddr(&mut raw, &[2, 5047, 13, -1, -1, -1, -1, -1]));
        assert!(pkt_getaddr(&raw).is_none());
    }
}
