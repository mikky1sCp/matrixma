//! CLI-майнер для MatrixMA.
//!
//! Примеры:
//!   matrixma-mine --header "hello" --bits 8
//!   matrixma-mine --header "block-42" --bits 16 --threads 8
//!   matrixma-mine --help

use std::env;
use std::io::Write;
use std::process::exit;
use std::time::Instant;

use matrixma::{target_from_bits, Context, Miner};

fn usage() -> ! {
    eprintln!("matrixma-mine — майнер MatrixMA");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  matrixma-mine --header <string> --bits <n> [--threads <n>]");
    eprintln!();
    eprintln!("Args:");
    eprintln!("  --header <string>   произвольная строка, хешируется как header блока");
    eprintln!("  --bits <n>          сколько ведущих нулевых бит в хеше (0..=64)");
    eprintln!("  --threads <n>       число потоков (0 = все ядра, по умолчанию)");
    eprintln!("  --help, -h          показать эту справку");
    exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut header: Option<String> = None;
    let mut bits: Option<u32> = None;
    let mut threads: usize = 0;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--header" => {
                header = Some(args.get(i + 1).cloned().unwrap_or_else(|| usage()));
                i += 2;
            }
            "--bits" => {
                let v = args.get(i + 1).unwrap_or_else(|| usage());
                bits = Some(v.parse().unwrap_or_else(|_| usage()));
                i += 2;
            }
            "--threads" => {
                let v = args.get(i + 1).unwrap_or_else(|| usage());
                threads = v.parse().unwrap_or_else(|_| usage());
                i += 2;
            }
            "--help" | "-h" => usage(),
            _ => usage(),
        }
    }

    let header = header.unwrap_or_else(|| usage());
    let bits = bits.unwrap_or_else(|| usage());

    if bits > 64 {
        eprintln!("error: --bits должен быть <= 64");
        exit(1);
    }

    let target = target_from_bits(bits);

    println!("Header:  {:?}", header);
    println!("Bits:    {}", bits);
    println!("Target:  {}", hex::encode(target));
    println!(
        "Threads: {}",
        if threads == 0 {
            "auto".to_string()
        } else {
            threads.to_string()
        }
    );
    println!();

    print!("Preparing context... ");
    std::io::stdout().flush().ok();
    let t0 = Instant::now();
    let ctx = Context::prepare(header.as_bytes());
    let prep = t0.elapsed();
    println!("done in {:.3} s", prep.as_secs_f64());
    println!();

    let miner = Miner::new(ctx, target);

    println!("Mining (Ctrl+C для остановки)...");
    let t1 = Instant::now();
    let sol = miner.mine_parallel(threads, u64::MAX);
    let dt = t1.elapsed();

    match sol {
        Some(s) => {
            println!();
            println!("Found!");
            println!("  Nonce: {}", s.nonce);
            println!("  Hash:  {}", hex::encode(s.hash));
            println!("  Time:  {:.3} s", dt.as_secs_f64());
            let attempts = s.nonce as f64 + 1.0;
            let rate = attempts / dt.as_secs_f64();
            println!("  Rate:  ~{:.1} H/s", rate);
        }
        None => {
            println!("No solution found");
            exit(2);
        }
    }
}