//! Кэшированный верификатор PoW с ограничением параллельных prepare.
//!
//! НЕ подключён к lib.rs по умолчанию — включается фичей `async-verifier`.
//! Требует tokio и lru. Предназначен для будущего P2P-слоя, где верификация
//! идёт асинхронно и нельзя блокировать executor на 1.1 с.
//!
//! # Зачем
//!
//! `Context::prepare` стоит ~1.1 с и ест 256 МБ RAM. Если на каждый
//! входящий блок делать prepare с нуля, нода захлебнётся под нагрузкой.
//! Кэш по хешу header'а переиспользует Context для повторных проверок
//! того же блока (от разных пиров, при реоргах, при рестарте).
//!
//! # Ограничения
//!
//! Кэш НЕ защищает от атаки уникальными header'ами. Это задача P2P-слоя:
//! не звать verify до дешёвых проверок (prev_hash, timestamp, merkle_root,
//! размер блока, rate limit per peer).
//!
//! # Пример
//!
//! ```no_run
//! # #[cfg(feature = "async-verifier")] {
//! use std::sync::Arc;
//! use matrixma::verifier::Verifier;
//!
//! # async fn example() {
//! let verifier = Arc::new(Verifier::new(2, 2));  // 2 Context'а, 2 prepare
//! let header = b"block-header";
//! let target = [0xffu8; 32];
//! let nonce = 0u64;
//!
//! if verifier.verify_async(&header, nonce, &target).await {
//!     println!("PoW валиден");
//! }
//! # }
//! # }
//! ```

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use lru::LruCache;
use tokio::sync::Semaphore;

use crate::Context;

pub struct Verifier {
    cache: Mutex<LruCache<[u8; 32], Arc<Context>>>,
    prepare_slots: Semaphore,
}

impl Verifier {
    /// `cache_size` — сколько Context'ов держать (каждый ~300 МБ RAM).
    /// `max_concurrent_prepare` — сколько prepare крутить одновременно.
    ///
    /// Разумные дефолты: `(2, 2)` для ноды на 4 ГБ RAM.
    pub fn new(cache_size: usize, max_concurrent_prepare: usize) -> Self {
        let cap = NonZeroUsize::new(cache_size.max(1)).unwrap();
        Self {
            cache: Mutex::new(LruCache::new(cap)),
            prepare_slots: Semaphore::new(max_concurrent_prepare.max(1)),
        }
    }

    /// Быстрый путь: только кэш, без блокировки.
    pub fn get(&self, header: &[u8]) -> Option<Arc<Context>> {
        let key = *blake3::hash(header).as_bytes();
        self.cache.lock().unwrap().get(&key).cloned()
    }

    /// Медленный путь: считает prepare и кладёт в кэш.
    /// Блокирующий — вызывать из spawn_blocking.
    fn prepare(&self, header: &[u8]) -> Arc<Context> {
        let key = *blake3::hash(header).as_bytes();
        let mut cache = self.cache.lock().unwrap();
        if let Some(c) = cache.get(&key) {
            return c.clone();
        }
        let ctx = Arc::new(Context::prepare(header));
        cache.put(key, ctx.clone());
        ctx
    }

    /// Синхронная проверка. Блокирует поток на ~1.1 с при промахе кэша.
    /// Для асинхронного контекста используйте `verify_async`.
    pub fn verify(&self, header: &[u8], nonce: u64, target: &[u8; 32]) -> bool {
        let ctx = match self.get(header) {
            Some(c) => c,
            None => self.prepare(header),
        };
        ctx.hash(nonce) < *target
    }

    /// Асинхронная проверка. `prepare` уходит в blocking pool.
    ///
    /// Семафор ограничивает число одновременных prepare, чтобы не забить CPU.
    /// При попадании в кэш семафор не трогается — fast path без await.
    ///
    /// Требует `Arc<Self>`, чтобы шарить между задачами.
    pub async fn verify_async(
        self: &Arc<Self>,
        header: &[u8],
        nonce: u64,
        target: &[u8; 32],
    ) -> bool {
        // Fast path: кэш-хит, никаких await.
        if let Some(ctx) = self.get(header) {
            return ctx.hash(nonce) < *target;
        }

        // Slow path: ограничиваем параллельные prepare.
        let _permit = self.prepare_slots.acquire().await.unwrap();

        // Double-check: пока ждали семафор, другой таск мог всё сделать.
        if let Some(ctx) = self.get(header) {
            return ctx.hash(nonce) < *target;
        }

        let header_owned = header.to_vec();
        let target_copy = *target;
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let ctx = this.prepare(&header_owned);
            ctx.hash(nonce) < target_copy
        })
        .await
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Vec<u8> {
        b"verifier-test-v1".to_vec()
    }

    fn easy_target() -> [u8; 32] {
        [0xffu8; 32]
    }

    fn impossible_target() -> [u8; 32] {
        [0x00u8; 32]
    }

    #[test]
    fn verify_sync_accepts_easy() {
        let v = Verifier::new(2, 1);
        assert!(v.verify(&header(), 0, &easy_target()));
    }

    #[test]
    fn verify_sync_rejects_impossible() {
        let v = Verifier::new(2, 1);
        assert!(!v.verify(&header(), 0, &impossible_target()));
    }

    #[test]
    fn cache_hit_after_first_verify() {
        let v = Verifier::new(2, 1);
        assert!(v.get(&header()).is_none());
        v.verify(&header(), 0, &easy_target());
        assert!(v.get(&header()).is_some());
    }

    #[test]
    fn different_headers_different_cache_entries() {
        let v = Verifier::new(2, 1);
        v.verify(b"a", 0, &easy_target());
        v.verify(b"b", 0, &easy_target());
        assert!(v.get(b"a").is_some());
        assert!(v.get(b"b").is_some());
    }

    #[test]
    fn cache_respects_size() {
        let v = Verifier::new(2, 1);
        v.verify(b"a", 0, &easy_target());
        v.verify(b"b", 0, &easy_target());
        v.verify(b"c", 0, &easy_target());
        // LRU вытеснил "a" — самый старый.
        assert!(v.get(b"a").is_none());
        assert!(v.get(b"b").is_some());
        assert!(v.get(b"c").is_some());
    }
}