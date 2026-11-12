//! Rung 5: replay a MAME trace on the core (the reference's `replay_file`) and print the
//! result as JSON, for `scripts/rung5.sh` to compare with the reference's on the same trace.
//!
//!     replay ROM ERROR_LOG [--cycles strict|report|off] [--limit N]

use serde_json::json;
use sm83::replay::{parse_trace, replay, CycleMode, START_PC};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut paths = Vec::new();
    let mut cycles = CycleMode::Strict;
    let mut limit = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cycles" => {
                cycles = match args.next().as_deref() {
                    Some("strict") => CycleMode::Strict,
                    Some("report") => CycleMode::Report,
                    Some("off") => CycleMode::Off,
                    other => panic!("unknown cycle mode {other:?}"),
                }
            }
            "--limit" => limit = Some(args.next().unwrap().parse().unwrap()),
            _ => paths.push(arg),
        }
    }
    assert!(
        paths.len() == 2,
        "usage: replay ROM ERROR_LOG [--cycles MODE] [--limit N]"
    );
    let rom = std::fs::read(&paths[0]).expect("read ROM");
    let log = std::fs::read(&paths[1]).expect("read log");
    let lines = parse_trace(&String::from_utf8_lossy(&log), START_PC);
    let result = replay(&rom, &lines, limit, cycles).expect("replay");
    let divergence = result
        .divergence
        .as_ref()
        .map(|d| json!({"position": d.position, "pc": d.line.pc, "problems": d.problems}));
    let value = json!({
        "compared": result.compared,
        "interrupts": result.interrupts,
        "skipped_cycle_checks": result.skipped_cycle_checks,
        "cycle_mismatches": result.cycle_mismatches,
        "divergence": divergence,
    });
    println!("{value}");
}
