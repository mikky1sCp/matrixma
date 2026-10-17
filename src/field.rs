//! Арифметика над конечным полем GF(p), p = 2^31 - 1 (простое Мерсенна).
//!
//! Все операции — строго над u64, без floating point.
//! Это критично для детерминизма: одинаковый результат на всех платформах.

use crate::params::P;

/// Приведение x по модулю p = 2^31 - 1 через трюк Мерсенна.
///
/// Идея: 2^31 ≡ 1 (mod p), поэтому x = hi·2^31 + lo ≡ hi + lo (mod p).
/// Повторяем, пока не получим число < p. Без деления.
#[inline(always)]
pub fn reduce(mut x: u64) -> u64 {
    x = (x & P) + (x >> 31);
    x = (x & P) + (x >> 31);
    if x >= P { x -= P; }
    x
}

/// Сложение по модулю p.
#[inline(always)]
pub fn add(a: u64, b: u64) -> u64 {
    debug_assert!(a < P && b < P);
    let s = a + b;
    if s >= P { s - P } else { s }
}

/// Вычитание по модулю p.
#[inline(always)]
pub fn sub(a: u64, b: u64) -> u64 {
    debug_assert!(a < P && b < P);
    if a >= b { a - b } else { a + P - b }
}

/// Умножение по модулю p.
#[inline(always)]
pub fn mul(a: u64, b: u64) -> u64 {
    debug_assert!(a < P && b < P);
    reduce(a * b)
}

/// Возведение в степень по модулю p (square-and-multiply).
pub fn pow(mut base: u64, mut exp: u64) -> u64 {
    let mut result = 1u64;
    base = reduce(base);
    while exp > 0 {
        if exp & 1 == 1 {
            result = mul(result, base);
        }
        base = mul(base, base);
        exp >>= 1;
    }
    result
}

/// Сложение с накоплением.
#[inline(always)]
pub fn add_assign(acc: &mut u64, a: u64) {
    *acc = add(*acc, a);
}

/// Умножение с накоплением.
#[inline(always)]
pub fn mul_add_assign(acc: &mut u64, a: u64, b: u64) {
    *acc = add(*acc, mul(a, b));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduce_boundaries() {
        assert_eq!(reduce(0), 0);
        assert_eq!(reduce(P - 1), P - 1);
        assert_eq!(reduce(P), 0);
        assert_eq!(reduce(P + 1), 1);
        assert_eq!(reduce(2 * P), 0);
        let max = u64::MAX;
        assert!(reduce(max) < P);
    }

    #[test]
    fn add_sub_roundtrip() {
        for a in [0, 1, 12345, P - 1, P / 2] {
            for b in [0, 1, 9999, P - 1] {
                let s = add(a, b);
                assert!(s < P);
                assert_eq!(sub(s, b), a);
            }
        }
    }

    #[test]
    fn mul_basic() {
        assert_eq!(mul(0, 12345), 0);
        assert_eq!(mul(1, 12345), 12345);
        assert_eq!(mul(P - 1, P - 1), 1);
    }

    #[test]
    fn pow_fermat() {
        for a in [1u64, 2, 3, 12345, P - 1] {
            assert_eq!(pow(a, P - 1), 1);
        }
    }

    #[test]
    fn mul_add_matches_manual() {
        let a = 123456789;
        let b = 987654321;
        let mut acc = 42;
        mul_add_assign(&mut acc, a, b);
        assert_eq!(acc, add(42, mul(a, b)));
    }
}