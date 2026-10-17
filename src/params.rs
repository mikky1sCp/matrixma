//! Все константы алгоритма MatrixMA. Меняются только здесь.

/// Размер квадратной матрицы N×N.
pub const N: usize = 768;

/// Простое число Мерсенна p = 2^31 - 1.
pub const P: u64 = (1 << 31) - 1;

/// Размер датасета для memory-hard слоя: 256 МБ.
pub const DATASET_SIZE: usize = 256 << 20;

/// Количество memory-hard шагов.
pub const MEMORY_STEPS: u32 = 262_144;

/// Раунды вероятностной проверки Фрейвалдса.
pub const FREIVALDS_ROUNDS: u8 = 3;