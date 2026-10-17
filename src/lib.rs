//! MatrixMA: memory-hard PoW-алгоритм над GF(2^31 - 1).
//!
//! # Быстрый старт
//!
//! Майнер:
//!
//! ```no_run
//! use matrixma::{Context, Miner, target_from_bits};
//!
//! let header = b"my-block-header";
//! let ctx = Context::prepare(header);
//! let target = target_from_bits(20);
//! let miner = Miner::new(ctx, target);
//!
//! if let Some(sol) = miner.mine_parallel(0, u64::MAX) {
//!     println!("найден nonce = {}, hash = {:?}", sol.nonce, sol.hash);
//! }
//! ```
//!
//! Нода:
//!
//! ```no_run
//! use matrixma::{verify, target_from_bits};
//!
//! let header = b"my-block-header";
//! let target = target_from_bits(20);
//! let nonce = 12345u64;
//!
//! if verify(header, nonce, &target) {
//!     println!("PoW валиден");
//! }
//! ```
//!
//! # Архитектура
//!
//! - `params` — константы алгоритма.
//! - `field` — арифметика над GF(2³¹ − 1).
//! - `hash` — BLAKE3 + ChaCha20.
//! - `matrix` — матрицы N×N и Фрейвалдс.
//! - `memory` — 256-МБ датасет и memory-hard walk.
//! - `miner` — многопоточный поиск nonce.
//! - `lib.rs` — фасад: `Context`, `hash`, `verify`, `target_from_bits`.

pub mod params;
pub mod field;
pub mod matrix;
pub mod memory;
pub mod hash;
pub mod miner;

pub use miner::{Miner, Solution};

use matrix::{generate_pair, mul, Matrix};
use memory::Dataset;
use params::{MEMORY_STEPS, N};

/// Подготовленный контекст для одного блока.
///
/// Держит seed, базовое состояние и 256-МБ датасет. Майнер создаёт
/// его один раз на блок и гоняет `hash(nonce)` в цикле.
///
/// `Context` реализует `Sync`: можно шарить между потоками через
/// `&self`. Это ключевое свойство для `Miner::mine_parallel`.
pub struct Context {
    seed: [u8; 32],
    base_state: u64,
    dataset: Dataset,
}

impl Context {
    /// Считает всё, что зависит только от header. Один раз на блок.
    pub fn prepare(header: &[u8]) -> Self {
        let seed = hash::final_hash(header);
        let (a, b) = generate_pair(&seed);
        let c = mul(&a, &b);
        let base_state = compress_matrix(&c);
        let dataset = Dataset::build(&seed);
        Context { seed, base_state, dataset }
    }

    /// Хеш для заданного nonce. Датасет и матрицы переиспользуются.
    ///
    /// Nonce расширяет стартовое состояние walk до полного u64 через XOR.
    /// Walk внутри работает с 64-битной арифметикой, так что различные
    /// nonce дают различные пути — пространство состояний 2⁶⁴, а не 2³¹.
    pub fn hash(&self, nonce: u64) -> [u8; 32] {
        let start = self.base_state ^ nonce;
        let final_state = self.dataset.walk(start, MEMORY_STEPS);

        let mut buf = [0u8; 40];
        buf[..32].copy_from_slice(&self.seed);
        buf[32..].copy_from_slice(&final_state.to_le_bytes());
        hash::final_hash(&buf)
    }

    /// Доступ к seed — для отладки и логирования.
    pub fn seed(&self) -> &[u8; 32] {
        &self.seed
    }
}

/// Одноразовый майнинг-хеш. Для реального майнинга используйте `Context`.
pub fn hash(header: &[u8], nonce: u64) -> [u8; 32] {
    Context::prepare(header).hash(nonce)
}

/// Проверка PoW.
pub fn verify(header: &[u8], nonce: u64, target: &[u8; 32]) -> bool {
    hash(header, nonce) < *target
}

/// Строит target по числу ведущих нулевых бит.
///
/// `target_from_bits(b)` возвращает такой `[u8; 32]`, что хеш проходит
/// проверку `hash < target` тогда и только тогда, когда у него
/// минимум `b` ведущих нулевых бит.
///
/// # Пример
///
/// ```
/// use matrixma::target_from_bits;
///
/// let t = target_from_bits(8);
/// assert_eq!(t[0], 0x00);
/// assert_eq!(t[1], 0xff);
///
/// let t0 = target_from_bits(0);
/// assert_eq!(t0, [0xffu8; 32]);
/// ```
pub fn target_from_bits(bits: u32) -> [u8; 32] {
    assert!(bits <= 256, "bits must be <= 256");
    if bits == 0 {
        return [0xffu8; 32];
    }
    if bits == 256 {
        return [0x00u8; 32];
    }

    let ones = 256 - bits;
    let full_bytes = (ones / 8) as usize;
    let rem_bits = (ones % 8) as u8;

    let mut target = [0u8; 32];
    for i in 0..full_bytes {
        target[31 - i] = 0xff;
    }
    if full_bytes < 32 && rem_bits > 0 {
        target[31 - full_bytes] = (1u8 << rem_bits) - 1;
    }
    target
}

/// Сжимает матрицу C в одно состояние < P через BLAKE3.
fn compress_matrix(c: &Matrix) -> u64 {
    let mut bytes = vec![0u8; N * N * 8];
    for (i, &x) in c.data.iter().enumerate() {
        bytes[i * 8..(i + 1) * 8].copy_from_slice(&x.to_le_bytes());
    }
    let h = hash::final_hash(&bytes);
    let v = u64::from_le_bytes(h[..8].try_into().unwrap());
    field::reduce(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Vec<u8> {
        b"test-block-header-v1".to_vec()
    }

    #[test]
    fn hash_deterministic() {
        let ctx = Context::prepare(&header());
        assert_eq!(ctx.hash(0), ctx.hash(0));
    }

    #[test]
    fn hash_nonce_changes() {
        let ctx = Context::prepare(&header());
        assert_ne!(ctx.hash(0), ctx.hash(1));
    }

    #[test]
    fn hash_header_changes() {
        let a = Context::prepare(b"header-a").hash(0);
        let b = Context::prepare(b"header-b").hash(0);
        assert_ne!(a, b);
    }

    #[test]
    fn context_reuse_matches_fresh() {
        let ctx = Context::prepare(&header());
        assert_eq!(ctx.hash(42), hash(&header(), 42));
    }

    #[test]
    fn hash_different_high_nonces() {
        // Nonce space 2^64, а не 2^31. Проверяем, что старшие биты nonce
        // действительно влияют на результат.
        let ctx = Context::prepare(&header());
        let a = ctx.hash(1u64 << 40);
        let b = ctx.hash((1u64 << 40) + 1);
        let c = ctx.hash((1u64 << 63) + 7);
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn hash_full_u64_nonce_space() {
        // Разница только в старших битах nonce должна давать разный хеш.
        let ctx = Context::prepare(&header());
        let h_low = ctx.hash(1);
        let h_high = ctx.hash((1u64 << 63) | 1);
        assert_ne!(h_low, h_high);
    }

    #[test]
    fn verify_accepts_valid() {
        assert!(verify(&header(), 0, &[0xffu8; 32]));
    }

    #[test]
    fn verify_rejects_impossible_target() {
        assert!(!verify(&header(), 0, &[0x00u8; 32]));
    }

    // --- target_from_bits ---

    #[test]
    fn target_from_bits_zero() {
        assert_eq!(target_from_bits(0), [0xffu8; 32]);
    }

    #[test]
    fn target_from_bits_eight() {
        let t = target_from_bits(8);
        assert_eq!(t[0], 0x00);
        for i in 1..32 {
            assert_eq!(t[i], 0xff);
        }
    }

    #[test]
    fn target_from_bits_four() {
        let t = target_from_bits(4);
        assert_eq!(t[0], 0x0f);
        for i in 1..32 {
            assert_eq!(t[i], 0xff);
        }
    }

    #[test]
    fn target_from_bits_256() {
        assert_eq!(target_from_bits(256), [0x00u8; 32]);
    }

    #[test]
    fn target_from_bits_monotonic() {
        let t8 = target_from_bits(8);
        let t16 = target_from_bits(16);
        let t24 = target_from_bits(24);
        assert!(t8 > t16);
        assert!(t16 > t24);
    }

    // --- тяжёлые, запускаются через --ignored ---

    #[test]
    #[ignore]
    fn full_pow_benchmark() {
        let h = header();

        let t0 = std::time::Instant::now();
        let ctx = Context::prepare(&h);
        let prep = t0.elapsed();

        let t1 = std::time::Instant::now();
        let _ = ctx.hash(0);
        let one = t1.elapsed();

        let t2 = std::time::Instant::now();
        for n in 0..10 {
            let _ = ctx.hash(n);
        }
        let ten = t2.elapsed();

        println!(
            "prepare = {:?}, one hash = {:?}, ten hashes = {:?}",
            prep, one, ten
        );

        assert!(one.as_millis() < 200, "один hash < 200 мс");
        assert!(ten.as_millis() < 2000, "десять hashes < 2 с");
    }

    /// Тест-вектор консенсуса.
    ///
    /// Запусти с --nocapture, возьми hex из вывода, впиши в assert.
    /// После этого любое изменение алгоритма сломает тест — и это правильно.
    ///
    /// cargo test --release --lib test_vector -- --ignored --nocapture
    /// Тест-вектор консенсуса.
    ///
    /// Значения получены на эталонной сборке с честным 64-битным
    /// nonce space. Если алгоритм меняется, эти assert'ы падают —
    /// сигнал, что все ноды должны быть обновлены синхронно (hardfork),
    /// иначе сеть расколется.
    ///
    /// cargo test --release --lib test_vector -- --ignored
    #[test]
    #[ignore]
    fn test_vector() {
        let h = b"matrixma-test-vector-v1";
        let h0 = hash(h, 0);
        let h1 = hash(h, 1);
        let h42 = hash(h, 42);

        println!("nonce=0  : {}", hex::encode(h0));
        println!("nonce=1  : {}", hex::encode(h1));
        println!("nonce=42 : {}", hex::encode(h42));

        assert_eq!(
            hex::encode(h0),
            "040e619defaab7ee6bc70f3b631e970480056506dcb649dfc6224b9bd0b29de4"
        );
        assert_eq!(
            hex::encode(h1),
            "92a49817c752fc101925c98b76e6f0a9ceeeb0771989031d5555956b494e8b85"
        );
        assert_eq!(
            hex::encode(h42),
            "a20453d1950049bdcbba4d5decc7103d83ca92ff7a95f693cecd57f90f125f7a"
        );
    }
}