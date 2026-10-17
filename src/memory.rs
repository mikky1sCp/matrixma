//! Memory-hard слой: большой датасет + длинный цикл случайного доступа.
//!
//! Датасет строится из seed (зависит от header, не от nonce) и живёт
//! между попытками майнинга. Это даёт ASIC-resistance.
//!
//! `walk` — сердце алгоритма. Каждый шаг читает 8 байт по случайному
//! индексу, зависящему от предыдущего состояния. Цепочка зависимостей
//! не даёт распараллелить шаги, а 256 МБ не влезают в кэш → каждая
//! итерация упирается в latency памяти.

use crate::field;
use crate::hash::expand;
use crate::params::DATASET_SIZE;

/// Большой массив случайных u64 над GF(p).
pub struct Dataset {
    data: Vec<u64>,
}

impl Dataset {
    /// Строит датасет стандартного размера (256 МБ).
    pub fn build(seed: &[u8; 32]) -> Self {
        Self::with_size(seed, DATASET_SIZE)
    }

    /// Строит датасет произвольного размера (байт, кратно 8 и степень двойки).
    pub fn with_size(seed: &[u8; 32], bytes: usize) -> Self {
        assert!(bytes % 8 == 0, "размер датасета должен быть кратен 8");
        assert!(bytes.is_power_of_two(), "размер датасета должен быть степенью двойки");

        let mut buf = vec![0u8; bytes];
        expand(seed, &mut buf);

        let n = bytes / 8;
        let mut data = Vec::with_capacity(n);
        for i in 0..n {
            let mut chunk = [0u8; 8];
            chunk.copy_from_slice(&buf[i * 8..i * 8 + 8]);
            data.push(field::reduce(u64::from_le_bytes(chunk)));
        }

        Dataset { data }
    }

    /// Количество элементов.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Один memory-hard проход.
    ///
    /// `state` — произвольный u64. Внутри — 64-битная wrapping-арифметика,
    /// чтобы сохранить полное пространство состояний (2⁶⁴, а не 2³¹).
    /// Раньше здесь стоял `field::reduce(state)`, который схлопывал state
    /// до 31 бита и убивал весь смысл 64-битного nonce.
    ///
    /// Каждый шаг:
    /// 1. индекс = младшие биты state (маска, len — степень двойки);
    /// 2. читаем data[idx] — промах кэша при 256 МБ;
    /// 3. state = state · data[idx] (wrapping u64);
    /// 4. XOR-fold — нелинейность, ломает линейные связи;
    /// 5. state += i · K — counter с золотым сечением, предотвращает зацикливание;
    /// 6. ещё один XOR-fold — финальная нелинейность.
    ///
    /// Зависимость каждого шага от предыдущего убивает параллелизм:
    /// GPU не может выполнить 256K шагов одновременно, они последовательны.
    #[inline]
    pub fn walk(&self, mut state: u64, steps: u32) -> u64 {
        let mask = self.data.len() - 1;
        // Константа золотого сечения — хорошо перемешивает counter.
        const COUNTER_MUL: u64 = 0x9E37_79B9_7F4A_7C15;
        for i in 0..steps {
            let idx = (state as usize) & mask;
            let d = self.data[idx];
            state = state.wrapping_mul(d);
            state ^= state >> 33;
            state = state.wrapping_add((i as u64).wrapping_mul(COUNTER_MUL));
            state ^= state >> 29;
        }
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::DATASET_SIZE;

    const TEST_BYTES: usize = 64 * 1024;
    const DATASET_LEN: usize = DATASET_SIZE / 8;

    fn seed() -> [u8; 32] {
        [7u8; 32]
    }

    #[test]
    fn dataset_deterministic() {
        let a = Dataset::with_size(&seed(), TEST_BYTES);
        let b = Dataset::with_size(&seed(), TEST_BYTES);
        assert_eq!(a.data, b.data);
    }

    #[test]
    fn dataset_different_seeds_differ() {
        let a = Dataset::with_size(&[1u8; 32], TEST_BYTES);
        let b = Dataset::with_size(&[2u8; 32], TEST_BYTES);
        assert_ne!(a.data, b.data);
    }

    #[test]
    fn dataset_elements_in_range() {
        let d = Dataset::with_size(&seed(), TEST_BYTES);
        for &x in &d.data {
            assert!(x < crate::params::P);
        }
    }

    #[test]
    fn dataset_len_correct() {
        let d = Dataset::with_size(&seed(), TEST_BYTES);
        assert_eq!(d.len(), TEST_BYTES / 8);
    }

    #[test]
    fn walk_deterministic() {
        let d = Dataset::with_size(&seed(), TEST_BYTES);
        let s1 = d.walk(12345, 1000);
        let s2 = d.walk(12345, 1000);
        assert_eq!(s1, s2);
    }

    #[test]
    fn walk_changes_state() {
        let d = Dataset::with_size(&seed(), TEST_BYTES);
        let s = 999;
        let out = d.walk(s, 100);
        assert_ne!(out, s);
    }

    #[test]
    fn walk_accepts_full_u64_state() {
        // state может быть любым u64 — walk сохраняет полное пространство.
        let d = Dataset::with_size(&seed(), TEST_BYTES);
        for s in [0u64, 1, 12345, (1u64 << 63), u64::MAX] {
            let _ = d.walk(s, 1000);
        }
    }

    #[test]
    fn walk_uses_full_state_space() {
        // Два u64, различающиеся только в старших битах, должны давать
        // разные результаты — иначе nonce space снова коллапсирует до 2^31.
        let d = Dataset::with_size(&seed(), TEST_BYTES);
        let a = d.walk(0x0000_0000_0000_0001, 5000);
        let b = d.walk(0x8000_0000_0000_0001, 5000);
        assert_ne!(a, b);

        // И ещё одна пара: различие только в бите 40.
        let c = d.walk(1u64 << 40, 5000);
        let e = d.walk((1u64 << 40) | 1, 5000);
        assert_ne!(c, e);
    }

    #[test]
    fn walk_sensitive_to_input() {
        let d = Dataset::with_size(&seed(), TEST_BYTES);
        let a = d.walk(100, 10000);
        let b = d.walk(101, 10000);
        assert_ne!(a, b);
    }

    #[test]
    fn walk_high_nonces_differ() {
        let d = Dataset::with_size(&seed(), TEST_BYTES);
        let a = d.walk(1u64 << 40, 5000);
        let b = d.walk((1u64 << 40) + 1, 5000);
        assert_ne!(a, b);
    }

    #[test]
    #[ignore]
    fn walk_real_size_smoke() {
        let d = Dataset::build(&seed());
        assert_eq!(d.len(), DATASET_LEN);
        let _ = d.walk(42, 262_144);
    }
}