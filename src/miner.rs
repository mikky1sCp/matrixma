//! Многопоточный майнер для MatrixMA.
//!
//! API:
//!   - `Miner::new(ctx, target)` — подготовка.
//!   - `miner.mine_single(start, max_nonce)` — один поток.
//!   - `miner.mine_parallel(threads, max_nonce)` — N потоков.
//!
//! Контекст шарится между потоками через `&self`. Dataset (256 МБ)
//! один на все потоки — это главный выигрыш многопоточности:
//! `prepare` делается один раз, а не N раз.

use crate::Context;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Найденное решение PoW.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Solution {
    /// Nonce, при котором хеш проходит target.
    pub nonce: u64,
    /// Сам хеш — для проверки и логирования.
    pub hash: [u8; 32],
}

/// Майнер: держит Context и target, умеет искать nonce.
pub struct Miner {
    ctx: Context,
    target: [u8; 32],
}

impl Miner {
    /// Создаёт майнер.
    ///
    /// `target` — верхняя граница хеша в big-endian. Хеш проходит,
    /// если `hash < target`. Для удобства есть `crate::target_from_bits`.
    pub fn new(ctx: Context, target: [u8; 32]) -> Self {
        Miner { ctx, target }
    }

    /// Доступ к контексту.
    pub fn context(&self) -> &Context {
        &self.ctx
    }

    /// Целевой хеш.
    pub fn target(&self) -> &[u8; 32] {
        &self.target
    }

    /// Однопоточный поиск nonce в диапазоне `[start, max_nonce)`.
    ///
    /// Возвращает `Some(Solution)` при успехе или `None`, если диапазон
    /// исчерпан.
    pub fn mine_single(&self, start: u64, max_nonce: u64) -> Option<Solution> {
        let mut nonce = start;
        while nonce < max_nonce {
            let h = self.ctx.hash(nonce);
            if h < self.target {
                return Some(Solution { nonce, hash: h });
            }
            nonce = match nonce.checked_add(1) {
                Some(n) => n,
                None => return None,
            };
        }
        None
    }

    /// Многопоточный поиск.
    ///
    /// `threads = 0` — использовать все доступные ядра.
    /// `max_nonce` — верхняя граница пространства поиска (не включительно).
    /// Обычно ставят `u64::MAX`, чтобы покрыть всё пространство.
    ///
    /// Пространство nonce делится между потоками: поток `t` проверяет
    /// nonce `t, t + threads, t + 2·threads, ...`. Это исключает
    /// дублирование работы.
    ///
    /// При первом найденном решении остальные потоки останавливаются
    /// через `AtomicBool` — они завершают текущий хеш (~27 мс) и выходят.
    pub fn mine_parallel(&self, threads: usize, max_nonce: u64) -> Option<Solution> {
        let threads = if threads == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        } else {
            threads
        };
        let stride = threads as u64;

        let stop = AtomicBool::new(false);
        let result: Mutex<Option<Solution>> = Mutex::new(None);

        let this = self;
        std::thread::scope(|s| {
            for tid in 0..threads {
                let stop = &stop;
                let result = &result;
                s.spawn(move || {
                    let mut nonce = tid as u64;
                    while nonce < max_nonce {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        let h = this.ctx.hash(nonce);
                        if h < this.target {
                            let mut guard = result.lock().unwrap();
                            if guard.is_none() {
                                *guard = Some(Solution { nonce, hash: h });
                            }
                            stop.store(true, Ordering::Relaxed);
                            return;
                        }
                        nonce = match nonce.checked_add(stride) {
                            Some(n) => n,
                            None => return,
                        };
                    }
                });
            }
        });

        result.into_inner().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Context;

    /// Общий target = max → первый же хеш проходит. Быстрый тест.
    fn easy_target() -> [u8; 32] {
        [0xffu8; 32]
    }

    /// Target = 0 → ничего не проходит.
    fn impossible_target() -> [u8; 32] {
        [0x00u8; 32]
    }

    fn header() -> Vec<u8> {
        b"miner-test-v1".to_vec()
    }

    #[test]
    fn mine_single_finds_easy_target() {
        let ctx = Context::prepare(&header());
        let miner = Miner::new(ctx, easy_target());
        let sol = miner.mine_single(0, 100).expect("easy target должен найтись");
        assert_eq!(sol.nonce, 0);
        assert!(sol.hash < easy_target());
    }

    #[test]
    fn mine_single_returns_none_for_impossible() {
        let ctx = Context::prepare(&header());
        let miner = Miner::new(ctx, impossible_target());
        // Ограничиваем 3 попытками — не хотим реально ждать.
        let sol = miner.mine_single(0, 3);
        assert!(sol.is_none());
    }

    #[test]
    fn mine_parallel_finds_easy_target() {
        let ctx = Context::prepare(&header());
        let miner = Miner::new(ctx, easy_target());
        let sol = miner
            .mine_parallel(4, 1000)
            .expect("easy target должен найтись и в параллельном режиме");
        assert!(sol.hash < easy_target());
    }

    #[test]
    fn mine_parallel_respects_thread_count() {
        // Не проверяем физически потоки, но проверяем, что API не падает
        // при threads = 1 и threads = 0 (auto).
        let ctx = Context::prepare(&header());
        let miner = Miner::new(ctx, easy_target());

        let sol1 = miner.mine_parallel(1, 100).expect("threads=1");
        assert!(sol1.hash < easy_target());

        let sol2 = miner.mine_parallel(0, 100).expect("threads=auto");
        assert!(sol2.hash < easy_target());
    }

    #[test]
    fn mine_parallel_returns_none_for_impossible() {
        let ctx = Context::prepare(&header());
        let miner = Miner::new(ctx, impossible_target());
        let sol = miner.mine_parallel(2, 4);
        assert!(sol.is_none());
    }

    #[test]
    fn solution_is_deterministic_per_input() {
        // Разные запуски на одинаковом inpute дают один и тот же nonce
        // (потому что hash детерминирован, и порядок обхода фиксирован).
        let ctx = Context::prepare(&header());
        let miner = Miner::new(ctx, easy_target());
        let s1 = miner.mine_parallel(1, 100).unwrap();
        let s2 = miner.mine_parallel(1, 100).unwrap();
        assert_eq!(s1, s2);
    }
}