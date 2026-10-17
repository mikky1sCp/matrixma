//! Общие криптографические обёртки: BLAKE3 и ChaCha20.
//!
//! Тут только то, что нужно и matrix.rs, и memory.rs, и lib.rs.
//! Никакой логики алгоритма — только детерминированные примитивы.

use blake3;
use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20;

/// Детерминированный поток байт из 32-байтового seed.
///
/// ChaCha20 как keystream: ключ = seed, nonce = 0.
#[inline]
pub fn expand(seed: &[u8; 32], out: &mut [u8]) {
    let mut cipher = ChaCha20::new(seed.into(), (&[0u8; 12]).into());
    cipher.apply_keystream(out);
}

/// Финальный хеш: BLAKE3 от произвольного буфера.
#[inline]
pub fn final_hash(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

/// Деривирует независимый seed для раунда через BLAKE3.
///
/// Простой XOR недостаточен: при round = 0 он вернул бы исходный seed,
/// а из этого же seed уже сгенерированы матрицы A и B через `expand`.
/// Тогда случайный вектор Фрейвалдса для раунда 0 оказался бы
/// коррелирован с A/B/C, и гарантия 1/P перестала бы работать.
#[inline]
pub fn mix_seed(seed: &[u8; 32], round: u8) -> [u8; 32] {
    let mut buf = [0u8; 33];
    buf[..32].copy_from_slice(seed);
    buf[32] = round;
    *blake3::hash(&buf).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_deterministic() {
        let s = [42u8; 32];
        let mut a = [0u8; 128];
        let mut b = [0u8; 128];
        expand(&s, &mut a);
        expand(&s, &mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn expand_different_seeds_differ() {
        let mut a = [0u8; 128];
        let mut b = [0u8; 128];
        expand(&[1u8; 32], &mut a);
        expand(&[2u8; 32], &mut b);
        assert_ne!(a, b);
    }

    #[test]
    fn final_hash_deterministic() {
        let h1 = final_hash(b"hello");
        let h2 = final_hash(b"hello");
        assert_eq!(h1, h2);
        assert_ne!(h1, final_hash(b"hellp"));
    }

    #[test]
    fn mix_seed_changes() {
        let s = [0u8; 32];
        assert_ne!(mix_seed(&s, 0), mix_seed(&s, 1));
        assert_ne!(mix_seed(&s, 0), s);
    }

    #[test]
    fn mix_seed_all_rounds_independent() {
        let s = [7u8; 32];
        let r0 = mix_seed(&s, 0);
        let r1 = mix_seed(&s, 1);
        let r2 = mix_seed(&s, 2);
        assert_ne!(r0, s);
        assert_ne!(r1, s);
        assert_ne!(r2, s);
        assert_ne!(r0, r1);
        assert_ne!(r1, r2);
        assert_ne!(r0, r2);
    }

    #[test]
    fn mix_seed_breaks_keystream_correlation() {
        let s = [42u8; 32];
        let mixed = mix_seed(&s, 0);

        let mut from_original = [0u8; 32];
        let mut from_mixed = [0u8; 32];
        expand(&s, &mut from_original);
        expand(&mixed, &mut from_mixed);

        assert_ne!(from_original, from_mixed);
    }
}