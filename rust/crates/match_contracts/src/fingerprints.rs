pub const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

#[inline]
pub fn start() -> u64 { OFFSET_BASIS }

pub fn mix_i32(mut hash: u64, value: i32) -> u64 {
    let value = value as u32;
    for shift in (0..32).step_by(8) {
        hash ^= (value >> shift & 0xff) as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

pub fn mix_i64(mut hash: u64, value: i64) -> u64 {
    let value = value as u64;
    for shift in (0..64).step_by(8) {
        hash ^= value >> shift & 0xff;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

pub fn mix_bool(hash: u64, value: bool) -> u64 {
    mix_i32(hash, if value {1} else {0})
}

pub fn mix_string(mut hash: u64, value: Option<&str>) -> u64 {
    match value {
        None => mix_i32(hash, -1),
        Some(s) => {
            let utf16: Vec<u16> = s.encode_utf16().collect();
            hash = mix_i32(hash, utf16.len() as i32);
            for unit in utf16 { hash = mix_i32(hash, unit as i32); }
            hash
        }
    }
}
