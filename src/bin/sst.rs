//! Rung 2: SingleStepTests `sm83`, 498 files x 1,000 cases (the reference's
//! `tests/sst_runner.py`, transcribed).
//!
//!     sst CORPUS_DIR [--limit N] [--jobs N]
//!
//! Each case gives initial and final registers, sparse RAM and one `[address, data, "rwm"]`
//! sample per M-cycle, the opcode fetch first. Reads and writes are compared in address, data
//! and order; internal cycles by kind only (the corpus writes the previous address there,
//! contradiction C1); `ie` is ignored and `ei` is compared on the final state (C4). HALT and
//! STOP (`76`, `10`) are excluded (C3). `$FFFF` cases seed `ie` from the case's RAM because
//! the core owns `$FFFF`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use serde::Deserialize;
use sm83::{Bus, CpuState, Sm83};

const EXCLUDED: [&str; 2] = ["10", "76"];

#[derive(Deserialize)]
struct Initial {
    pc: u16,
    sp: u16,
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    h: u8,
    l: u8,
    ime: u8,
    ram: Vec<(u16, u8)>,
}

#[derive(Deserialize)]
struct Final {
    pc: u16,
    sp: u16,
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    h: u8,
    l: u8,
    ime: u8,
    #[serde(default)]
    ei: u8,
    ram: Vec<(u16, u8)>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    initial: Initial,
    #[serde(rename = "final")]
    final_: Final,
    cycles: Vec<(u16, Option<u8>, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Read,
    Write,
    Internal,
}

/// The cycle kind a corpus `rwm` flag string records.
fn kind_of(flags: &str) -> Kind {
    let bytes = flags.as_bytes();
    if bytes[0] == b'r' {
        Kind::Read
    } else if bytes[1] == b'w' {
        Kind::Write
    } else {
        Kind::Internal
    }
}

/// A 64 KiB RAM host that records every bus callback of a step.
struct Bench {
    memory: Vec<u8>,
    log: Vec<(Kind, u16, Option<u8>)>,
}

impl Bus for Bench {
    fn read(&mut self, address: u16, _cpu: &mut Sm83) -> u8 {
        let value = self.memory[address as usize];
        self.log.push((Kind::Read, address, Some(value)));
        value
    }

    fn write(&mut self, address: u16, value: u8, _cpu: &mut Sm83) {
        self.memory[address as usize] = value;
        self.log.push((Kind::Write, address, Some(value)));
    }

    fn idle(&mut self, address: u16, _cpu: &mut Sm83) {
        self.log.push((Kind::Internal, address, None));
    }
}

/// Run one case; return the list of disagreements (empty when it passes).
fn run_case(bench: &mut Bench, cpu: &mut Sm83, case: &Case) -> Vec<String> {
    let (initial, final_) = (&case.initial, &case.final_);
    for &(address, value) in &initial.ram {
        bench.memory[address as usize] = value;
    }
    bench.log.clear();
    // The core owns $FFFF and reads it back as IE; the corpus treats it as RAM and ignores
    // its own `ie` field, so IE starts as whatever the case has at $FFFF.
    let ie = initial
        .ram
        .iter()
        .rev()
        .find(|(a, _)| *a == 0xFFFF)
        .map(|(_, v)| *v)
        .unwrap_or(0);
    cpu.restore_state(&CpuState {
        a: initial.a,
        b: initial.b,
        c: initial.c,
        d: initial.d,
        e: initial.e,
        f: initial.f,
        h: initial.h,
        l: initial.l,
        pc: initial.pc,
        sp: initial.sp,
        ime: initial.ime != 0,
        ie,
        ..CpuState::default()
    });
    let mut problems = Vec::new();
    let cycles = cpu.step(bench);
    if cycles as usize != bench.log.len() {
        problems.push(format!(
            "step() returned {cycles} but made {} callbacks",
            bench.log.len()
        ));
    }
    let expected: Vec<(Kind, u16, Option<u8>)> = case
        .cycles
        .iter()
        .map(|(address, data, flags)| (kind_of(flags), *address, *data))
        .collect();
    for (index, (want, got)) in expected.iter().zip(bench.log.iter()).enumerate() {
        if want.0 != got.0 || (want.0 != Kind::Internal && (want.1, want.2) != (got.1, got.2)) {
            problems.push(format!(
                "cycle {}: expected {want:?}, got {got:?}",
                index + 1
            ));
        }
    }
    if expected.len() != bench.log.len() {
        problems.push(format!(
            "{} cycles, expected {}",
            bench.log.len(),
            expected.len()
        ));
    }
    let state = cpu.capture_state();
    let pairs: [(&str, u16, u16); 10] = [
        ("a", state.a as u16, final_.a as u16),
        ("b", state.b as u16, final_.b as u16),
        ("c", state.c as u16, final_.c as u16),
        ("d", state.d as u16, final_.d as u16),
        ("e", state.e as u16, final_.e as u16),
        ("f", state.f as u16, final_.f as u16),
        ("h", state.h as u16, final_.h as u16),
        ("l", state.l as u16, final_.l as u16),
        ("pc", state.pc, final_.pc),
        ("sp", state.sp, final_.sp),
    ];
    for (name, got, want) in pairs {
        if got != want {
            problems.push(format!("{name}: {got:#x} != {want:#x}"));
        }
    }
    if state.ime != (final_.ime != 0) {
        problems.push(format!("ime: {} != {}", state.ime, final_.ime != 0));
    }
    if state.ei_pending != (final_.ei != 0) {
        problems.push(format!("ei: {} != {}", state.ei_pending, final_.ei != 0));
    }
    let wanted: std::collections::HashMap<u16, u8> = final_.ram.iter().copied().collect();
    let mut wanted_sorted: Vec<_> = wanted.iter().collect();
    wanted_sorted.sort();
    for (&address, &value) in wanted_sorted {
        let have = bench.memory[address as usize];
        if have != value {
            problems.push(format!("[{address:#06x}] = {have:#x}, expected {value:#x}"));
        }
    }
    for &(kind, address, _) in &bench.log {
        if kind == Kind::Write && !wanted.contains_key(&address) {
            problems.push(format!("unexpected write to {address:#06x}"));
        }
    }
    let mut touched: HashSet<u16> = initial.ram.iter().map(|(a, _)| *a).collect();
    touched.extend(wanted.keys());
    touched.extend(bench.log.iter().map(|(_, a, _)| *a));
    for address in touched {
        bench.memory[address as usize] = 0;
    }
    problems
}

/// Run a corpus file (or its first `limit` cases); return (cases, `name: problem` lines).
fn run_file(path: &Path, limit: Option<usize>) -> (usize, Vec<String>) {
    let text = std::fs::read_to_string(path).expect("read corpus file");
    let cases: Vec<Case> = serde_json::from_str(&text).expect("parse corpus file");
    let mut bench = Bench {
        memory: vec![0; 0x10000],
        log: Vec::new(),
    };
    let mut cpu = Sm83::new();
    let mut failures = Vec::new();
    let count = limit.unwrap_or(cases.len()).min(cases.len());
    for case in &cases[..count] {
        failures.extend(
            run_case(&mut bench, &mut cpu, case)
                .into_iter()
                .map(|p| format!("{}: {p}", case.name)),
        );
    }
    (count, failures)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut corpus: Option<PathBuf> = None;
    let mut limit = None;
    let mut jobs = 8usize;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--limit" => limit = Some(args.next().unwrap().parse().unwrap()),
            "--jobs" => jobs = args.next().unwrap().parse().unwrap(),
            _ => corpus = Some(PathBuf::from(arg)),
        }
    }
    let corpus = corpus.expect("usage: sst CORPUS_DIR [--limit N] [--jobs N]");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&corpus)
        .expect("read corpus directory")
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "json")
                && !EXCLUDED.contains(&p.file_stem().unwrap().to_str().unwrap())
        })
        .collect();
    files.sort();
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = files.get(index) else { break };
                let outcome = run_file(path, limit);
                results.lock().unwrap().push((path.clone(), outcome));
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by(|a, b| a.0.cmp(&b.0));
    let (mut cases, mut failed_cases, mut failed_files) = (0usize, 0usize, 0usize);
    for (path, (count, failures)) in &results {
        cases += count;
        if !failures.is_empty() {
            failed_files += 1;
            let names: HashSet<&str> = failures
                .iter()
                .map(|f| f.split(':').next().unwrap())
                .collect();
            failed_cases += names.len();
            println!(
                "{}: {} disagreements",
                path.file_name().unwrap().to_string_lossy(),
                failures.len()
            );
            for line in failures.iter().take(5) {
                println!("  {line}");
            }
        }
    }
    println!(
        "{} files, {} cases: {} passed, {} failed ({} files with failures)",
        results.len(),
        cases,
        cases - failed_cases,
        failed_cases,
        failed_files
    );
    if failed_cases != 0 {
        std::process::exit(1);
    }
}
