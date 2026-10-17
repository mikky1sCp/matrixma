# MatrixMA

Memory-hard proof-of-work алгоритм над GF(2³¹ − 1) для приватного блокчейна.
CPU-friendly, ASIC-resistant, детерминированный.

**Версия:** v0.1.0-pow (алгоритм и майнер)
**Статус:** работает, покрыт тестами, готов к тестнету

---

## Что это

PoW-модуль, который превращает `(header, nonce)` в 32-байтовый хеш
так, чтобы его было **выгодно считать на CPU и невыгодно на GPU/ASIC**.

Ключевые свойства:

- **Memory-hard.** 256 МБ датасета + 262 144 шага последовательного
  random access. GPU не может спрятать latency, ASIC не может зашить
  датасет в кремний — он зависит от header и меняется каждый блок.
- **Детерминированный.** Только целочисленная арифметика, никаких
  floating point. Test-vector зафиксирован, любое изменение ломает тест.
- **Честный 64-битный nonce.** Пространство поиска 2⁶⁴, а не 2³¹.
- **Быстрый на CPU.** ~270 H/s на 8 ядрах, ~26 мс на хеш.
- **Быстрая проверка.** ~1.1 с на блок при 12-секундном блок-тайме.

---

## Архитектура алгоритма

```
header, nonce
      │
      ▼
   BLAKE3  ──►  seed (32 байта, зависит только от header)
      │
      ├──► ChaCha20(seed) → A, B  (768×768 над GF(p))
      │            │
      │            ▼
      │       C = A · B mod p
      │            │
      │            ▼
      │       BLAKE3(C) → base_state (u64, < p)
      │
      └──► ChaCha20(seed) → Dataset (256 МБ)

start = base_state ^ nonce
final = walk(start, 262144 шагов, Dataset)
hash  = BLAKE3(seed || final)
```

Майнер вызывает `walk` миллионы раз с разными `nonce`, переиспользуя
`seed`, `A`, `B`, `C` и `Dataset`. `prepare` (~1.1 с) делается один
раз на блок, `hash` (~26 мс) — на каждый nonce.

---

## Установка

Нужен Rust 1.75+:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Или на Windows — установщик с [rustup.rs](https://rustup.rs).

Сборка:

```bash
cargo build --release
```

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

Preparing context... done in 1.157 s

Mining (Ctrl+C для остановки)...

Found!
  Nonce: 320
  Hash:  007e835ffb80b467528f29c24866c17af10b2223d6e4f5bb4d064ff0bb442d43
  Time:  1.180 s
  Rate:  ~272.0 H/s
```

Опции:
- `--header <string>` — header блока (или тестовая строка)
- `--bits <n>` — сколько ведущих нулевых бит требуется (0..=64)
- `--threads <n>` — число потоков (0 = все ядра)

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

Verifying... done in 1.153 s

VALID
```

Exit code: `0` — валидно, `1` — невалидно, `2` — ошибка аргументов.

### Библиотека

```rust
use matrixma::{Context, Miner, target_from_bits, verify};

// Майнер
let header = b"my-block-header";
let ctx = Context::prepare(header);           // ~1.1 с, один раз на блок
let target = target_from_bits(16);            // 16 ведущих нулевых бит
let miner = Miner::new(ctx, target);

if let Some(sol) = miner.mine_parallel(0, u64::MAX) {
    println!("nonce = {}", sol.nonce);
    println!("hash  = {}", hex::encode(sol.hash));

    // Проверка (можно на другой машине)
    assert!(verify(header, sol.nonce, &target));
}
```

---

## Тесты

Быстрые (46 штук, ~6 с):

```bash
cargo test --release --lib
```

Тяжёлые (256-МБ датасет, бенчмарк, консенсус):

```bash
cargo test --release --lib -- --ignored
```

Test-vector консенсуса:

```bash
cargo test --release --lib test_vector -- --ignored
```

Если этот тест падает — **алгоритм изменился**. Все ноды сети должны
обновиться синхронно (hardfork), иначе сеть расколется.

Бенчмарк:

```bash
cargo test --release --lib full_pow_benchmark -- --ignored --nocapture
```

---

## Метрики (8 ядер, Ryzen 5 / аналог)

| Параметр | Значение |
|---|---|
| `prepare` (один раз на блок) | ~1.14 с |
| `hash` (на каждый nonce) | ~25.7 мс |
| Скорость майнинга | ~272 H/s |
| Датасет | 256 МБ |
| Матрицы | 2 × 768 × 768 × 8 байт |
| RAM майнера | ~300 МБ |

---

## Структура

```
matrixma/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs        # фасад: Context, hash, verify, target_from_bits
    ├── params.rs     # константы алгоритма
    ├── field.rs      # GF(2³¹ − 1), Мерсенн
    ├── hash.rs       # BLAKE3 + ChaCha20 обёртки
    ├── matrix.rs     # матрицы N×N, Фрейвалдс
    ├── memory.rs     # 256-МБ датасет, memory-hard walk
    ├── miner.rs      # многопоточный поиск nonce
    └── bin/
        ├── matrixma-mine.rs
        └── matrixma-verify.rs
```

---

## Безопасность

**Не аудирован.** Это исследовательский прототип, не production-ready
криптография. Перед запуском mainnet нужны:

1. Кросс-платформенная проверка test-vector (Linux, macOS, ARM).
2. Независимый аудит `walk`, `compress_matrix`, `mix_seed`.
3. Экономический анализ: 51%-атака при малом хешрейте.
4. Формальный анализ time-memory tradeoff на walk.

Для тестнета и обучения — годится. Для денег — нет.

---

## Лицензия

MIT (или выбери свою — сейчас не указана).
```

---

## `.gitignore`

```
/target
Cargo.lock.bak
*.swp
.DS_Store
```

`Cargo.lock` **не игнорируем** — это библиотека + бинарники, для них lock-файл должен быть в репозитории (для воспроизводимых сборок и консенсуса).

---

## `rust-toolchain.toml`

Фиксирует версию Rust — важно для консенсуса, чтобы все ноды собирали одной версией:

```toml
[toolchain]
channel = "1.75.0"
components = ["rustfmt", "clippy"]
```

Если у тебя установлена другая версия и не хочешь ставить 1.75 — замени `channel` на `"stable"` или `"1.80.0"`. Главное — зафиксировать, чтобы не «поплыло» на другой машине.

---

## `LICENSE` (если MIT)

```
MIT License

Copyright (c) 2026 <твоё имя>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## Git-команды

```powershell
cd C:\Users\user\Desktop\matrixma

# Проверить, что git установлен
git --version

# Если репозиторий уже инициализирован — пропусти init
git init

# Настроить имя/email, если ещё не делал глобально
git config user.name "Твоё Имя"
git config user.email "you@example.com"

# Добавить всё
git add .

# Первый коммит
git commit -m "v0.1.0-pow: MatrixMA algorithm, miner, verifier, test-vector"

# Тег
git tag -a v0.1.0-pow -m "PoW module: memory-hard, CPU-friendly, ASIC-resistant"

# Проверить
git log --oneline
git tag
```

Ожидаемый вывод:

```
$ git log --oneline
a1b2c3d v0.1.0-pow: MatrixMA algorithm, miner, verifier, test-vector

$ git tag
v0.1.0-pow
```

---

## Проверка перед коммитом

Одна команда — все тесты + бенчмарк:

```powershell
cargo test --release --lib
cargo test --release --lib test_vector -- --ignored
cargo test --release --lib full_pow_benchmark -- --ignored --nocapture
```
