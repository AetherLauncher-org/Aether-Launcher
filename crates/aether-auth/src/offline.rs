//! Offline ("cracked") account support.
//!
//! A launcher that cannot reach the Microsoft identity platform still has to give the
//! game a profile: a display name, a stable identity and a deterministic UUID. Vanilla
//! Java derives that UUID from the player name with
//! [`UUID.nameUUIDFromBytes`](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/UUID.html#nameUUIDFromBytes(byte%5B%5D)),
//! which is MD5 over the UTF-8 bytes of `OfflinePlayer:<name>` with the version and
//! variant bits patched in. Servers rely on that value to keep inventories and
//! permissions attached to the same player across launchers, so this module has to
//! reproduce it bit for bit.
//!
//! The crate does not depend on a digest library (see the dependency list in
//! `Cargo.toml`), so [`md5`] contains a self-contained RFC 1321 implementation. It is
//! not a general-purpose hashing API: the only thing that matters here is that it
//! agrees with the published MD5 test vectors, which the test module asserts.

use uuid::Uuid;

use crate::{AccountId, AccountProfile};

/// Shortest accepted offline player name, in characters.
pub const MIN_PLAYER_NAME_LEN: usize = 3;

/// Longest accepted offline player name, in characters.
pub const MAX_PLAYER_NAME_LEN: usize = 16;

/// The prefix vanilla Java prepends to the player name before hashing.
const UUID_PREFIX: &str = "OfflinePlayer:";

/// Reasons a player name is rejected for offline use.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OfflineError {
    #[error("the player name must be 3 to 16 characters long")]
    NameLength,
    #[error("the player name may only contain ASCII letters, digits and underscores")]
    NameCharacters,
}

/// Checks that `name` is a usable offline player name.
///
/// Minecraft counts these names in characters, not bytes. The accepted alphabet is
/// ASCII-only, so the character count and the byte count are always equal; the check
/// is still written in character terms so it matches the platform rule, and every
/// non-ASCII input is rejected rather than measured differently.
///
/// # Errors
///
/// Returns [`OfflineError::NameLength`] when the name is shorter than
/// [`MIN_PLAYER_NAME_LEN`] or longer than [`MAX_PLAYER_NAME_LEN`] characters, and
/// [`OfflineError::NameCharacters`] when it contains anything outside `[A-Za-z0-9_]`.
pub fn validate_player_name(name: &str) -> Result<(), OfflineError> {
    let length = name.chars().count();
    if !(MIN_PLAYER_NAME_LEN..=MAX_PLAYER_NAME_LEN).contains(&length) {
        return Err(OfflineError::NameLength);
    }
    // Checking per character rather than per byte keeps the rule identical to the
    // regex `[A-Za-z0-9_]` applied to the whole string: any multi-byte character
    // fails `is_ascii_alphanumeric` and `'_'`, so it is rejected instead of being
    // split into bytes that might individually look allowed.
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(OfflineError::NameCharacters);
    }
    Ok(())
}

/// Returns whether [`validate_player_name`] accepts `name`.
#[must_use]
pub fn is_valid_player_name(name: &str) -> bool {
    validate_player_name(name).is_ok()
}

/// Derives the offline UUID vanilla Java assigns to a player called `name`.
///
/// This is exactly `UUID.nameUUIDFromBytes("OfflinePlayer:<name>".getBytes(UTF_8))`:
/// MD5 of the prefixed name, then the UUID version is forced to 3 and the variant to
/// RFC 4122.
///
/// # Errors
///
/// Returns the [`OfflineError`] produced by [`validate_player_name`]; no UUID is
/// derived for a name that would be rejected by the game.
pub fn offline_uuid(name: &str) -> Result<Uuid, OfflineError> {
    validate_player_name(name)?;

    let mut prefixed = String::with_capacity(UUID_PREFIX.len() + name.len());
    prefixed.push_str(UUID_PREFIX);
    prefixed.push_str(name);

    let mut digest = md5::digest(prefixed.as_bytes());
    // Version 3 (name based, MD5) lives in the high nibble of byte 6.
    digest[6] = (digest[6] & 0x0f) | 0x30;
    // RFC 4122 variant: the two most significant bits of byte 8 are `10`.
    digest[8] = (digest[8] & 0x3f) | 0x80;

    Ok(Uuid::from_bytes(digest))
}

/// Builds the offline profile for `name`.
///
/// The display name is stored exactly as given, so callers are responsible for any
/// trimming they want; validation only decides whether the name is legal.
///
/// # Errors
///
/// Returns the [`OfflineError`] produced by [`offline_uuid`].
pub fn offline_account(name: &str) -> Result<AccountProfile, OfflineError> {
    let id = AccountId::from_uuid(offline_uuid(name)?);
    Ok(AccountProfile::offline(id, name))
}

/// A hand-written RFC 1321 MD5 implementation.
///
/// Only used to reproduce Minecraft's name-based UUIDs, which are specified in terms
/// of MD5. It is deliberately not exported and does not attempt to be constant time:
/// nothing here handles secrets.
mod md5 {
    /// Per-round left-rotation amounts, in the message-schedule order of RFC 1321.
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, //
        5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, //
        4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, //
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];

    /// `T[i] = floor(2^32 * abs(sin(i + 1)))`, as listed in RFC 1321.
    const SINE_TABLE: [u32; 64] = [
        0xd76a_a478,
        0xe8c7_b756,
        0x2420_70db,
        0xc1bd_ceee,
        0xf57c_0faf,
        0x4787_c62a,
        0xa830_4613,
        0xfd46_9501,
        0x6980_98d8,
        0x8b44_f7af,
        0xffff_5bb1,
        0x895c_d7be,
        0x6b90_1122,
        0xfd98_7193,
        0xa679_438e,
        0x49b4_0821,
        0xf61e_2562,
        0xc040_b340,
        0x265e_5a51,
        0xe9b6_c7aa,
        0xd62f_105d,
        0x0244_1453,
        0xd8a1_e681,
        0xe7d3_fbc8,
        0x21e1_cde6,
        0xc337_07d6,
        0xf4d5_0d87,
        0x455a_14ed,
        0xa9e3_e905,
        0xfcef_a3f8,
        0x676f_02d9,
        0x8d2a_4c8a,
        0xfffa_3942,
        0x8771_f681,
        0x6d9d_6122,
        0xfde5_380c,
        0xa4be_ea44,
        0x4bde_cfa9,
        0xf6bb_4b60,
        0xbebf_bc70,
        0x289b_7ec6,
        0xeaa1_27fa,
        0xd4ef_3085,
        0x0488_1d05,
        0xd9d4_d039,
        0xe6db_99e5,
        0x1fa2_7cf8,
        0xc4ac_5665,
        0xf429_2244,
        0x432a_ff97,
        0xab94_23a7,
        0xfc93_a039,
        0x655b_59c3,
        0x8f0c_cc92,
        0xffef_f47d,
        0x8584_5dd1,
        0x6fa8_7e4f,
        0xfe2c_e6e0,
        0xa301_4314,
        0x4e08_11a1,
        0xf753_7e82,
        0xbd3a_f235,
        0x2ad7_d2bb,
        0xeb86_d391,
    ];

    /// Initial chaining state: `A`, `B`, `C`, `D` from RFC 1321.
    const INITIAL_STATE: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];

    /// The 16 message-word selectors of each round, in the order RFC 1321 lists them.
    const WORD_INDEX: [[usize; 16]; 4] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [1, 6, 11, 0, 5, 10, 15, 4, 9, 14, 3, 8, 13, 2, 7, 12],
        [5, 8, 11, 14, 1, 4, 7, 10, 13, 0, 3, 6, 9, 12, 15, 2],
        [0, 7, 14, 5, 12, 3, 10, 1, 8, 15, 6, 13, 4, 11, 2, 9],
    ];

    /// Returns the MD5 digest of `input` as 16 little-endian packed bytes.
    pub(super) fn digest(input: &[u8]) -> [u8; 16] {
        let mut state = INITIAL_STATE;
        let mut message = Vec::with_capacity(input.len() + 72);
        message.extend_from_slice(input);

        // Padding: a single `0x80` byte, then zeroes until the length is 56 modulo 64,
        // then the original length in bits as a little-endian u64.
        message.push(0x80);
        while message.len() % 64 != 56 {
            message.push(0);
        }
        let bit_len = (input.len() as u64).wrapping_mul(8);
        message.extend_from_slice(&bit_len.to_le_bytes());

        for chunk in message.as_chunks::<64>().0 {
            compress(&mut state, chunk);
        }

        let mut out = [0_u8; 16];
        for (word, slot) in state.iter().zip(out.as_chunks_mut::<4>().0) {
            slot.copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    /// Runs the 64 steps of one 512-bit block over `state`.
    fn compress(state: &mut [u32; 4], chunk: &[u8]) {
        let mut words = [0_u32; 16];
        for (word, slot) in words.iter_mut().zip(chunk.as_chunks::<4>().0) {
            *word = u32::from_le_bytes(*slot);
        }

        let [mut a, mut b, mut c, mut d] = *state;

        for (index, (shift, constant)) in SHIFTS.iter().zip(SINE_TABLE.iter()).enumerate() {
            let step = index % 16;
            let round = index / 16;
            // `step` is in `0..16`, so every lookup below is in bounds.
            let (f, word_slot) = match round {
                0 => ((b & c) | (!b & d), WORD_INDEX[0][step]),
                1 => ((d & b) | (!d & c), WORD_INDEX[1][step]),
                2 => (b ^ c ^ d, WORD_INDEX[2][step]),
                _ => (c ^ (b | !d), WORD_INDEX[3][step]),
            };
            let word = words.get(word_slot).copied().unwrap_or(0);

            let mixed = a
                .wrapping_add(f)
                .wrapping_add(word)
                .wrapping_add(*constant)
                .rotate_left(*shift)
                .wrapping_add(b);

            // Rotate the registers: `d` becomes the new `a`, `a` the new `b`, etc.
            a = d;
            d = c;
            c = b;
            b = mixed;
        }

        for (slot, value) in state.iter_mut().zip([a, b, c, d]) {
            *slot = slot.wrapping_add(value);
        }
    }
}

#[cfg(test)]
#[path = "offline_tests.rs"]
mod tests;
