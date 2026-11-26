//! Authoritative discrete-event match engine.
pub mod clock;
pub mod fingerprints;
pub mod rng;
pub mod contracts;
pub mod player_intelligence;
pub mod v4;
#[cfg(test)]
mod shared_regressions {
    use super::*;
    #[test]
    fn rng_matches_java_v317_exactly() {
        let cases: &[(i64, i64, i32, bool, u64, i64)] = &[
            (1, -7995527694508729151, 259, false, 0x3fdc7061a43b90b2, -7569645462458333503),
            (42, -4767286540954276203, 145, false, 0x3fd607387fc392b8, -7039776493598168826),
            (123456789, 2466975172287755897, 383, true, 0x3fe0a3c0c2c2d4b7, -8702667843733205468),
            (-27, -5313833362726698233, 412, true, 0x3fd96261e6c8b20c, -5552465284146965337),
        ];
        for &(seed, long, int, boolean, double_bits, fork) in cases {
            let mut r = rng::DeterministicRng::new(seed);
            assert_eq!(r.next_long(), long);
            assert_eq!(r.next_int(1000), int);
            assert_eq!(r.next_boolean(), boolean);
            assert_eq!(r.next_double().to_bits(), double_bits);
            assert_eq!(rng::derive_seed(seed, "BÌNH LUẬN 🎙️"), fork);
        }
    }
    #[test]
    fn shared_string_fingerprint_is_unchanged() {
        let mut h = fingerprints::start();
        h = fingerprints::mix_string(h, Some("Xin chào 🌍"));
        h = fingerprints::mix_i32(h, 123);
        h = fingerprints::mix_i64(h, -18);
        assert_eq!(h as i64, 2449983695915557691);
    }
}
