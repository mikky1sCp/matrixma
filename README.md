# MatrixMA

**CPU-friendly memory-hard proof-of-work для публичного блокчейна.**

Алгоритм, который делает майнинг выгодным на обычных процессорах и
невыгодным на GPU/ASIC. Плюс майнер, верификатор и CLI-инструменты.

**Версия:** v0.1.1-pow
**Статус:** PoW-модуль готов и покрыт тестами. Блокчейн — в разработке.

---

## Что это

MatrixMA — это **алгоритм PoW**, а не блокчейн (пока). Он превращает
`(header, nonce)` в 32-байтовый хеш так, чтобы:

- Считать было **выгодно на CPU** и невыгодно на GPU/ASIC.
- Результат был **детерминирован** — одинаковый на всех платформах.
- Проверка была **быстрой** — ~430 мс на блок.
- Nonce space был **честным** — 2⁶⁴, а не 2³¹.

Блокчейн (Block, Chain, DAA, транзакции) строится поверх этого модуля.
Спецификация консенсуса — в [`CONSENSUS.md`](CONSENSUS.md).

---

## Быстрый старт

### Установка Rust

Нужен Rust 1.75+:

- **Windows / macOS / Linux:** установщик с [rustup.rs](https://rustup.rs)
- **Linux одной командой:**
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

Проверка:
```bash
rustc --version
cargo --version
```

### Сборка

```bash
git clone <url>
cd matrixma
cargo build --release
```

Первый build скачает зависимости (~30 секунд).

---

## Использование

### CLI: майнер

```bash
matrixma-mine --header "hello" --bits 8
```

Вывод:
```
Header:  "hello"
Bits:    8
Target:  00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
Threads: auto

Preparing context... done in 0.431 s

Mining (Ctrl+C для остановки)...

Found!
  Nonce: 320
  Hash:  007e835ffb80b467528f29c24866c17af10b2223d6e4f5bb4d064ff0bb442d43
  Time:  1.180 s
  Rate:  ~272.0 H/s
```

| Опция | Смысл |
|---|---|
| `--header <string>` | header блока или тестовая строка |
| `--bits <n>` | ведущих нулевых бит требуется (0..=64) |
| `--threads <n>` | число потоков (0 = все ядра) |

`--bits` задаёт сложность:
- `8` — мгновенно (~1 секунда)
- `16` — ~5 минут на 8 ядрах
- `20` — ~1 час

### CLI: верификатор

```bash
matrixma-verify --header "hello" --nonce 320 --bits 8
```

Вывод:
```
Header:  "hello"
Nonce:   320
Bits:    8
Target:  00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

Verifying... done in 0.431 s

VALID
```

Exit codes: `0` — валидно, `1` — невалидно, `2` — ошибка аргументов.

### Библиотека

```rust
use matrixma::{Context, Miner, target_from_bits, verify};

// Майнер
let header = b"my-block-header";
let ctx = Context::prepare(header);        // ~430 мс, один раз на блок
let target = target_from_bits(16);         // 16 ведущих нулевых бит
let miner = Miner::new(ctx, target);

if let Some(sol) = miner.mine_parallel(0, u64::MAX) {
    println!("nonce = {}", sol.nonce);
    println!("hash  = {}", hex::encode(sol.hash));

    // Проверка (можно на другой машине)
    assert!(verify(header, sol.nonce, &target));
}
```

---

## Как это работает

### Архитектура алгоритма

```
header, nonce
      │
      ▼
   BLAKE3  ──►  seed (32 байта, зависит только от header)
      │
      ├──► ChaCha20(seed) → A, B  (768×768 над GF(p))
      │            │
      │            ▼
      │       C = A · B mod p          ← параллельно через Rayon
      │            │
      │            ▼
      │       BLAKE3(C) → base_state
      │
      └──► ChaCha20(seed) → Dataset (256 МБ)

start = base_state ^ nonce
final = walk(start, 262144 шагов, Dataset)
hash  = BLAKE3(seed || final)
```

### Почему это работает

**Memory-hard.** 256 МБ датасета + 262 144 последовательных шагов
случайного доступа. GPU не может спрятать latency памяти — она не
параллелится, потому что каждый шаг зависит от предыдущего.

**ASIC-resistant.** Датасет зависит от header и меняется каждый блок.
ASIC не может зашить его в кремний — придётся строить
перепрограммируемую память, что дорого и медленно.

**Детерминированный.** Только целочисленная арифметика над GF(2³¹ − 1),
никаких float. Один вход → один выход на всех платформах.

**Честный nonce space.** `walk` работает с 64-битной арифметикой.
Различных путей — 2⁶⁴, а не 2³¹.

### Что внутри

| Файл | Роль |
|---|---|
| `params.rs` | Константы алгоритма |
| `field.rs` | Арифметика над GF(2³¹ − 1), Мерсенн |
| `hash.rs` | BLAKE3 + ChaCha20 обёртки |
| `matrix.rs` | Матрицы N×N, Фрейвалдс, параллельное умножение |
| `memory.rs` | 256-МБ датасет, memory-hard walk |
| `miner.rs` | Многопоточный поиск nonce |
| `verifier.rs` | Кэшированный верификатор (feature `async-verifier`) |
| `lib.rs` | Фасад: `Context`, `hash`, `verify`, `target_from_bits` |

---

## Метрики

**8 ядер, Ryzen 5 / аналог:**

| Параметр | Значение |
|---|---|
| `prepare` (один раз на блок, параллельно) | ~431 мс |
| `hash` (на каждый nonce) | ~26–31 мс |
| Скорость майнинга | ~270–310 H/s |
| Датасет | 256 МБ |
| Матрицы | 2 × 768 × 768 × 8 байт |
| RAM майнера | ~300 МБ |

**Сравнение с другими PoW:**

| Алгоритм | CPU | GPU | ASIC | Память |
|---|---|---|---|---|
| SHA-256 (Bitcoin) | 10 MH/s | 10 GH/s | 100 TH/s | ~0 |
| RandomX (Monero) | 5 KH/s | 500 KH/s | нет | 2 ГБ |
| **MatrixMA** | **300 H/s** | **~1 KH/s** | **нет** | **256 МБ** |

MatrixMA в ~17 раз медленнее RandomX по CPU — это цена компактности
(6 файлов вместо десятков) и BLAKE3-сжатия. Но GPU-преимущество
всё равно сведено к ~3× вместо ~1000× у SHA-256.

---

## Тесты

### Быстрые (46 штук, ~5 секунд)

```bash
cargo test --release --lib
```

Проверяют:
- арифметику поля (`field`);
- детерминизм генерации (`hash`, `matrix`, `memory`);
- Фрейвалдс (принимает корректное, отвергает испорченное);
- nonce space (`walk_uses_full_state_space`);
- `target_from_bits` для разных диапазонов;
- майнер (single и parallel).

### Тяжёлые (`--ignored`, ~5 секунд)

```bash
cargo test --release --lib -- --ignored
```

- `walk_real_size_smoke` — реальный 256-МБ датасет.
- `test_vector` — консенсус. **Если падает — алгоритм изменился.**
- `full_pow_benchmark` — метрики.

### Test-vector консенсуса

```bash
cargo test --release --lib test_vector -- --ignored
```

Если этот тест проходит — значит алгоритм даёт **побитово
одинаковый результат** с эталонной сборкой. Любое изменение
`field.rs`, `hash.rs`, `matrix.rs`, `memory.rs` сломает его. Это
**правильно**: если тест падает, значит консенсус изменился, и
нужен хардфорк.

### Бенчмарк

```bash
cargo test --release --lib full_pow_benchmark -- --ignored --nocapture
```

Показывает `prepare / one hash / ten hashes`. Полезно при
оптимизациях — убедиться, что не стало хуже.

### Verifier (feature)

```bash
cargo test --release --features async-verifier verifier
```

Проверяет кэшированный верификатор: LRU-вытеснение, попадания,
промахи. Требует `tokio` и `lru`, которые включаются фичей.

---

## Что НЕ сделано

Это **исследовательский прототип**, не production-ready. Перед
запуском mainnet нужно:

1. **Кросс-платформенная проверка.** Test-vector должен дать те же
   hex-строки на Linux, macOS, ARM. Проверено только на Windows x86_64.
2. **Независимый аудит.** `walk`, `compress_matrix`, `mix_seed`,
   Фрейвалдс — формально не доказаны.
3. **Анализ time-memory tradeoff.** Возможна ли атака с меньшим
   объёмом памяти ценой большего времени? Не исследовано.
4. **Экономическая модель.** 51%-атака при хешрейте сети в 1 MH/s
   стоит копейки. Нужны checkpoint'ы или гибрид с PoS.
5. **Формат блока с commitment'ом.** Сейчас верификатор пересчитывает
   `C` целиком (~430 мс). Commitment на `C` + Freivalds-доказательство
   снизит до ~50 мс, но это изменение формата блока → хардфорк.

**Для обучения, тестнета и экспериментов — годится. Для денег — нет.**

---

## Структура проекта

```
matrixma/
├── Cargo.toml
├── README.md              ← этот файл
├── LICENSE                ← MIT
└── src/
    ├── lib.rs             ← фасад
    ├── params.rs          ← константы
    ├── field.rs           ← GF(2³¹ − 1)
    ├── hash.rs            ← BLAKE3 + ChaCha20
    ├── matrix.rs          ← матрицы N×N, Rayon
    ├── memory.rs          ← 256-МБ датасет, walk
    ├── miner.rs           ← многопоточный майнер
    ├── verifier.rs        ← кэшированный верификатор
    └── bin/
        ├── matrixma-mine.rs
        └── matrixma-verify.rs
```
## Лицензия

MIT. См. [`LICENSE`](LICENSE).
