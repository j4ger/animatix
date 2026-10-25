//! FxHash — rustc's fast, non-cryptographic hasher — for the environment
//! maps.
//!
//! Environment keys are build-time-fixed interpreter strings (`t`,
//! `scene.center.x`, `box0.size`), hashed and looked up thousands of times
//! per frame by the frame-env build and the modifier IR VM. They are not
//! attacker-controlled map keys, so the DoS-resistant SipHash default buys
//! nothing on the hottest maps in the frame path; FxHash is ~3-5× cheaper
//! per operation on short strings.

use std::hash::{BuildHasherDefault, Hasher};

use super::Value;

const SEED: u64 = 0x517c_c1b7_2722_0a95;

#[derive(Default)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add_to_hash(&mut self, i: u64) {
        self.hash = (self.hash.rotate_left(5) ^ i).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() >= 8 {
            self.add_to_hash(u64::from_ne_bytes(bytes[..8].try_into().expect("8-byte slice")));
            bytes = &bytes[8..];
        }
        if bytes.len() >= 4 {
            self.add_to_hash(
                u32::from_ne_bytes(bytes[..4].try_into().expect("4-byte slice")) as u64
            );
            bytes = &bytes[4..];
        }
        for &b in bytes {
            self.add_to_hash(b as u64);
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add_to_hash(i as u64);
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add_to_hash(i as u64);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add_to_hash(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add_to_hash(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

/// The environment's `HashMap<String, Value>` flavor: std `hashbrown` with
/// FxHash instead of SipHash.
pub type EnvMap = std::collections::HashMap<String, Value, BuildHasherDefault<FxHasher>>;
