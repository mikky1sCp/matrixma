//! Матрицы N×N над GF(p): генерация, умножение, проверка Фрейвалдса.
//!
//! Все операции детерминированы: один и тот же seed → одна и та же матрица
//! на любой платформе. Никаких floating point, только целые числа.

use crate::field;
use crate::hash::{expand, mix_seed};
use crate::params::{FREIVALDS_ROUNDS, N};

/// Квадратная матрица N×N над GF(p), row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matrix {
    pub data: Vec<u64>,
}

impl Matrix {
    /// Нулевая матрица.
    pub fn zeros() -> Self {
        Matrix { data: vec![0u64; N * N] }
    }

    /// Читает элемент (r, c).
    #[inline(always)]
    pub fn get(&self, r: usize, c: usize) -> u64 {
        self.data[r * N + c]
    }

    /// Записывает элемент (r, c). Значение должно быть < P.
    #[inline(always)]
    pub fn set(&mut self, r: usize, c: usize, v: u64) {
        debug_assert!(v < crate::params::P);
        self.data[r * N + c] = v;
    }
}

/// 8 байт → элемент поля.
#[inline(always)]
fn bytes_to_elem(b: &[u8]) -> u64 {
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&b[..8]);
    field::reduce(u64::from_le_bytes(buf))
}

/// Генерирует пару матриц (A, B) из одного seed.
pub fn generate_pair(seed: &[u8; 32]) -> (Matrix, Matrix) {
    let elems = N * N;
    let mut bytes = vec![0u8; elems * 8 * 2];
    expand(seed, &mut bytes);

    let mut a = Matrix::zeros();
    let mut b = Matrix::zeros();
    let a_bytes = &bytes[..elems * 8];
    let b_bytes = &bytes[elems * 8..];
    for i in 0..elems {
        a.data[i] = bytes_to_elem(&a_bytes[i * 8..]);
        b.data[i] = bytes_to_elem(&b_bytes[i * 8..]);
    }
    (a, b)
}

/// Умножение матриц: C = A · B mod P. Порядок i-k-j — cache-friendly.
pub fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    let mut c = Matrix::zeros();
    for i in 0..N {
        for k in 0..N {
            let aik = a.data[i * N + k];
            if aik == 0 {
                continue;
            }
            let b_row = &b.data[k * N..(k + 1) * N];
            let c_row = &mut c.data[i * N..(i + 1) * N];
            for j in 0..N {
                field::mul_add_assign(&mut c_row[j], aik, b_row[j]);
            }
        }
    }
    c
}

/// Матрица × вектор: out = M · v mod P. O(N²).
fn mat_vec(m: &Matrix, v: &[u64]) -> Vec<u64> {
    debug_assert_eq!(v.len(), N);
    let mut out = vec![0u64; N];
    for i in 0..N {
        let mut acc = 0u64;
        let row = &m.data[i * N..(i + 1) * N];
        for j in 0..N {
            field::mul_add_assign(&mut acc, row[j], v[j]);
        }
        out[i] = acc;
    }
    out
}

/// Случайный вектор длины N, детерминированно выведенный из seed и раунда.
fn random_vector(seed: &[u8; 32], round: u8) -> Vec<u64> {
    let mixed = mix_seed(seed, round);
    let mut bytes = vec![0u8; N * 8];
    expand(&mixed, &mut bytes);
    let mut v = Vec::with_capacity(N);
    for i in 0..N {
        v.push(bytes_to_elem(&bytes[i * 8..]));
    }
    v
}

/// Вероятностная проверка C = A · B методом Фрейвалдса.
pub fn freivalds_check(a: &Matrix, b: &Matrix, c: &Matrix, seed: &[u8; 32]) -> bool {
    for round in 0..FREIVALDS_ROUNDS {
        let r = random_vector(seed, round);
        let br = mat_vec(b, &r);
        let abr = mat_vec(a, &br);
        let cr = mat_vec(c, &r);
        if abr != cr {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field;
    use crate::params::P;

    fn seed() -> [u8; 32] {
        [42u8; 32]
    }

    #[test]
    fn generate_pair_deterministic() {
        let (a1, b1) = generate_pair(&seed());
        let (a2, b2) = generate_pair(&seed());
        assert_eq!(a1, a2);
        assert_eq!(b1, b2);
    }

    #[test]
    fn generate_pair_a_ne_b() {
        let (a, b) = generate_pair(&seed());
        assert_ne!(a, b);
    }

    #[test]
    fn generate_elements_in_range() {
        let (a, b) = generate_pair(&seed());
        for &x in a.data.iter().chain(b.data.iter()) {
            assert!(x < P);
        }
    }

    #[test]
    fn mul_by_identity() {
        let (a, _) = generate_pair(&seed());
        let mut i = Matrix::zeros();
        for k in 0..N {
            i.set(k, k, 1);
        }
        assert_eq!(mul(&i, &a), a);
    }

    #[test]
    fn freivalds_accepts_correct() {
        let s = seed();
        let (a, b) = generate_pair(&s);
        let c = mul(&a, &b);
        assert!(freivalds_check(&a, &b, &c, &s));
    }

    #[test]
    fn freivalds_rejects_wrong() {
        let s = seed();
        let (a, b) = generate_pair(&s);
        let mut c = mul(&a, &b);
        let v = c.get(0, 0);
        c.set(0, 0, field::add(v, 1));
        assert!(!freivalds_check(&a, &b, &c, &s));
    }
}