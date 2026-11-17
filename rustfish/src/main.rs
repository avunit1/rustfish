use std::env;
use rustfish::benchmark::run_benchmark;
use rustfish::uci::uci_loop;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1] == "bench" {
        let tt_size = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(16);
        let threads = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
        let depth = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(13);
        run_benchmark(tt_size, threads, depth);
    } else {
        uci_loop();
    }
}
