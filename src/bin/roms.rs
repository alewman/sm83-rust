//! Rung 3: blargg `cpu_instrs` 01, 03-11 and the Mooneye CPU ROMs in the test-ROM harness,
//! natively (the reference's `tests/test_blargg.py` and `tests/test_mooneye.py`).
//!
//!     roms BLARGG_INDIVIDUAL_DIR MOONEYE_ACCEPTANCE_DIR
//!
//! Each blargg ROM must end in a jump to itself with serial text ending "Passed"; 02 needs
//! the timer, which the harness does not have, and must fail the documented way ("Timer
//! doesn't work", "Failed #4"). Each Mooneye ROM must reach `LD B,B` with B,C,D,E,H,L =
//! 3,5,8,13,21,34.

use std::path::{Path, PathBuf};

use sm83::harness::Harness;

const MOONEYE: [&str; 11] = [
    "instr/daa",
    "bits/reg_f",
    "interrupts/ie_push",
    "if_ie_registers",
    "ei_sequence",
    "ei_timing",
    "rapid_di_ei",
    "intr_timing",
    "reti_intr_timing",
    "div_timing",
    "pop_timing",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: roms BLARGG_INDIVIDUAL_DIR MOONEYE_ACCEPTANCE_DIR");
        std::process::exit(2);
    }
    let (blargg, mooneye) = (Path::new(&args[0]), Path::new(&args[1]));
    let mut roms: Vec<PathBuf> = std::fs::read_dir(blargg)
        .expect("read blargg directory")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "gb"))
        .collect();
    roms.sort();
    let (mut passed, mut total) = (0, 0);
    for rom in &roms {
        let name = rom.file_stem().unwrap().to_string_lossy().into_owned();
        let result = Harness::new(&std::fs::read(rom).unwrap(), None).run(400_000_000);
        let ok = if name.starts_with("02-") {
            result.text.contains("Timer doesn't work") && result.text.contains("Failed #4")
        } else {
            result.reason == "jump to self" && result.text.trim_end().ends_with("Passed")
        };
        let last = result
            .text
            .trim_end()
            .lines()
            .last()
            .unwrap_or("")
            .to_string();
        println!(
            "blargg {name}: {} ({}, {} instructions, {} M-cycles, last line {last:?})",
            if ok { "ok" } else { "FAIL" },
            result.reason,
            result.instructions,
            result.cycles
        );
        total += 1;
        passed += ok as u32;
    }
    for test in MOONEYE {
        let path = mooneye.join(format!("{test}.gb"));
        let result = Harness::new(&std::fs::read(&path).unwrap(), None).run(10_000_000);
        let ok = result.mooneye_passed();
        println!(
            "mooneye {test}: {} ({}, {} instructions, {} M-cycles, registers {:?})",
            if ok { "ok" } else { "FAIL" },
            result.reason,
            result.instructions,
            result.cycles,
            result.registers
        );
        total += 1;
        passed += ok as u32;
    }
    println!("{passed} of {total} ROMs pass");
    if passed != total {
        std::process::exit(1);
    }
}
