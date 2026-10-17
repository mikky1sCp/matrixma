//! CLI-верификатор MatrixMA.
//!
//! Проверяет, что (header, nonce, bits) даёт валидный PoW.
//! Выход: 0 — валидно, 1 — невалидно, 2 — ошибка аргументов.
//!
//! Примеры:
//!   matrixma-verify --header "hello" --nonce 9668 --bits 12
//!   matrixma-verify --help

use std::env;
use std::process::exit;

use matrixma::{target_from_bits, verify};

fn usage() -> ! {
    eprintln!("matrixma-verify — верификатор PoW MatrixMA");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  matrixma-verify --header <string> --nonce <n> --bits <n>");
    eprintln!();
    eprintln!("Args:");
    eprintln!("  --header <string>   тот же header, что использовал майнер");
    eprintln!("  --nonce <n>         найденный nonce (u64)");
    eprintln!("  --bits <n>          сколько ведущих нулевых бит требуется (0..=64)");
    eprintln!("  --help, -h          показать эту справку");
    eprintln!();
    eprintln!("Exit codes:");
    eprintln!("  0 — PoW валиден");
    eprintln!("  1 — PoW невалиден");
    eprintln!("  2 — ошибка аргументов");
    exit(2);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut header: Option<String> = None;
    let mut nonce: Option<u64> = None;
    let mut bits: Option<u32> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--header" => {
                header = Some(args.get(i + 1).cloned().unwrap_or_else(|| usage()));
                i += 2;
            }
            "--nonce" => {
                let v = args.get(i + 1).unwrap_or_else(|| usage());
                nonce = Some(v.parse().unwrap_or_else(|_| usage()));
                i += 2;
            }
            "--bits" => {
                let v = args.get(i + 1).unwrap_or_else(|| usage());
                bits = Some(v.parse().unwrap_or_else(|_| usage()));
                i += 2;
            }
            "--help" | "-h" => usage(),
            _ => usage(),
        }
    }

    let header = header.unwrap_or_else(|| usage());
    let nonce = nonce.unwrap_or_else(|| usage());
    let bits = bits.unwrap_or_else(|| usage());

    if bits > 64 {
        eprintln!("error: --bits должен быть <= 64");
        exit(2);
    }

    let target = target_from_bits(bits);

    println!("Header:  {:?}", header);
    println!("Nonce:   {}", nonce);
    println!("Bits:    {}", bits);
    println!("Target:  {}", hex::encode(target));
    println!();

    print!("Verifying... ");
    std::io::Write::flush(&mut std::io::stdout()).ok();
    let t0 = std::time::Instant::now();
    let ok = verify(header.as_bytes(), nonce, &target);
    let dt = t0.elapsed();
    println!("done in {:.3} s", dt.as_secs_f64());
    println!();

    if ok {
        println!("VALID");
        exit(0);
    } else {
        println!("INVALID");
        exit(1);
    }
}