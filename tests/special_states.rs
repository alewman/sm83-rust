//! The reference's `tests/test_special_states.py`, transcribed: dispatch, HALT, EI/DI,
//! STOP, the holes, $FFFF. These run in CI; rung 4 compares the same scenarios with the
//! reference itself, by hand.

use sm83::{Bus, CpuState, Sm83, StopAction, StopResult};

type Hook = Box<dyn FnMut(usize, &mut Sm83)>;

/// 64 KiB RAM with a log of (kind, address, idu) and an optional per-cycle hook.
struct Host {
    memory: Vec<u8>,
    log: Vec<(&'static str, u16, i8)>,
    on_cycle: Option<Hook>,
    stop: Option<StopResult>,
    seen: Vec<bool>,
}

impl Host {
    fn new() -> Self {
        Host {
            memory: vec![0; 0x10000],
            log: Vec::new(),
            on_cycle: None,
            stop: None,
            seen: Vec::new(),
        }
    }

    fn load(&mut self, address: u16, data: &[u8]) {
        let start = address as usize;
        self.memory[start..start + data.len()].copy_from_slice(data);
    }

    fn record(&mut self, kind: &'static str, address: u16, cpu: &mut Sm83) {
        self.log.push((kind, address, cpu.idu));
        let n = self.log.len();
        if let Some(hook) = self.on_cycle.as_mut() {
            hook(n, cpu);
        }
    }
}

impl Bus for Host {
    fn read(&mut self, address: u16, cpu: &mut Sm83) -> u8 {
        self.record("read", address, cpu);
        self.memory[address as usize]
    }

    fn write(&mut self, address: u16, value: u8, cpu: &mut Sm83) {
        self.record("write", address, cpu);
        self.memory[address as usize] = value;
    }

    fn idle(&mut self, address: u16, cpu: &mut Sm83) {
        self.record("internal", address, cpu);
    }

    fn stop(&mut self, pending: bool, _cpu: &mut Sm83) -> StopResult {
        self.seen.push(pending);
        self.stop.unwrap_or(StopResult::new(StopAction::Stop))
    }
}

fn host_at(state: CpuState) -> (Sm83, Host) {
    let mut cpu = Sm83::new();
    cpu.restore_state(&state);
    (cpu, Host::new())
}

fn at(pc: u16) -> CpuState {
    CpuState {
        pc,
        sp: 0xC100,
        ..CpuState::default()
    }
}

fn run(cpu: &mut Sm83, host: &mut Host, steps: usize) -> Vec<u32> {
    (0..steps).map(|_| cpu.step(host)).collect()
}

#[test]
fn holes_lock_the_cpu_and_nothing_wakes_it() {
    for opcode in [
        0xD3, 0xDB, 0xDD, 0xE3, 0xE4, 0xEB, 0xEC, 0xED, 0xF4, 0xFC, 0xFD,
    ] {
        let (mut cpu, mut host) = host_at(CpuState {
            ie: 1,
            if_: 0,
            ime: true,
            ..at(0x0200)
        });
        host.load(0x0200, &[opcode]);
        assert_eq!(cpu.step(&mut host), 1);
        assert!(cpu.locked);
        cpu.if_ = 0x01;
        let pc = cpu.pc;
        host.log.clear();
        assert_eq!(run(&mut cpu, &mut host, 5), vec![1; 5]);
        assert_eq!(host.log, vec![("internal", 0x0000, 0); 5]);
        assert_eq!((cpu.pc, cpu.sp), (pc, 0xC100));
    }
}

#[test]
fn dispatch_is_five_cycles_with_the_documented_bus() {
    let (mut cpu, mut host) = host_at(CpuState {
        pc: 0x1234,
        ie: 0x04,
        if_: 0x04,
        ime: true,
        ..at(0)
    });
    assert_eq!(cpu.step(&mut host), 5);
    assert_eq!(
        host.log,
        vec![
            ("read", 0x1234, 1),
            ("internal", 0x1235, -1),
            ("internal", 0xC100, -1),
            ("write", 0xC0FF, -1),
            ("write", 0xC0FE, 0),
        ]
    );
    assert_eq!((host.memory[0xC0FF], host.memory[0xC0FE]), (0x12, 0x34));
    let s = cpu.capture_state();
    assert_eq!((s.pc, s.sp, s.ime, s.if_), (0x50, 0xC0FE, false, 0));
}

#[test]
fn vector_is_the_lowest_pending_bit() {
    for bit in 0..5u8 {
        let if_ = if bit < 4 { (1 << bit) | 0x10 } else { 0x10 };
        let (mut cpu, mut host) = host_at(CpuState {
            ie: 0x1F,
            if_,
            ime: true,
            ..at(0x0200)
        });
        cpu.step(&mut host);
        assert_eq!(cpu.pc, 0x40 + 8 * bit as u16);
    }
}

#[test]
fn ie_push_cancels_the_dispatch_and_sends_pc_to_zero() {
    let (mut cpu, mut host) = host_at(CpuState {
        pc: 0,
        sp: 0,
        ie: 1,
        if_: 1,
        ime: true,
        ..CpuState::default()
    });
    assert_eq!(cpu.step(&mut host), 5);
    let s = cpu.capture_state();
    assert_eq!((s.pc, s.ie, s.if_, s.ime), (0, 0, 1, false));
    let (mut cpu, mut host) = host_at(CpuState {
        pc: 0x0100,
        sp: 0,
        ie: 0x10,
        if_: 0x11,
        ime: true,
        ..CpuState::default()
    });
    cpu.step(&mut host);
    assert_eq!((cpu.pc, cpu.if_, cpu.ie), (0x40, 0x10, 0x01));
}

#[test]
fn requests_during_the_low_push() {
    // a request in the low push's callback can pick the vector
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 0x05,
        if_: 0x04,
        ime: true,
        ..at(0x0200)
    });
    host.on_cycle = Some(Box::new(|n, cpu| {
        if n == 5 {
            cpu.request_interrupt(0, false, false)
        }
    }));
    cpu.step(&mut host);
    assert_eq!((cpu.pc, cpu.if_), (0x40, 0x04));
    // a late one misses it
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 0x06,
        if_: 0x04,
        ime: true,
        ..at(0x0200)
    });
    host.on_cycle = Some(Box::new(|n, cpu| {
        if n == 5 {
            cpu.request_interrupt(1, false, true)
        }
    }));
    cpu.step(&mut host);
    assert_eq!((cpu.pc, cpu.if_), (0x50, 0x02));
    // a late rise of the bit being dispatched is a new edge
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 0x02,
        if_: 0x02,
        ime: true,
        ..at(0x0200)
    });
    host.on_cycle = Some(Box::new(|n, cpu| {
        if n == 5 {
            cpu.request_interrupt(1, false, true)
        }
    }));
    cpu.step(&mut host);
    assert_eq!((cpu.pc, cpu.if_), (0x48, 0x02));
    // in another cycle, `late` changes nothing
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 0x06,
        if_: 0x04,
        ime: true,
        ..at(0x0200)
    });
    host.on_cycle = Some(Box::new(|n, cpu| {
        if n == 4 {
            cpu.request_interrupt(1, false, true)
        }
    }));
    cpu.step(&mut host);
    assert_eq!((cpu.pc, cpu.if_, cpu.unsampled), (0x48, 0x04, None));
}

#[test]
fn a_request_during_a_step_is_seen_by_the_next_step_only() {
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 1,
        ime: true,
        ..at(0x0200)
    });
    host.on_cycle = Some(Box::new(|n, cpu| {
        if n == 1 {
            cpu.request_interrupt(0, false, false)
        }
    }));
    assert_eq!(cpu.step(&mut host), 1);
    assert_eq!(cpu.step(&mut host), 5);
}

#[test]
#[should_panic(expected = "0..4")]
fn request_interrupt_rejects_other_bits() {
    Sm83::new().request_interrupt(5, false, false);
}

#[test]
fn ei_di_reti() {
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 1,
        if_: 1,
        ..at(0x0200)
    });
    host.load(0x0200, &[0xFB, 0x00, 0x00]);
    assert_eq!(cpu.step(&mut host), 1);
    assert!(cpu.ei_pending && !cpu.ime);
    assert_eq!(cpu.step(&mut host), 1);
    assert!(cpu.ime && cpu.pc == 0x0202);
    assert_eq!(cpu.step(&mut host), 5);
    assert_eq!(cpu.pc, 0x40);

    let (mut cpu, mut host) = host_at(CpuState {
        ie: 1,
        if_: 1,
        ..at(0x0200)
    });
    host.load(0x0200, &[0xFB, 0xF3, 0x00]);
    assert_eq!(run(&mut cpu, &mut host, 3), vec![1, 1, 1]);
    assert!(!cpu.ime && cpu.pc == 0x0203);

    let (mut cpu, mut host) = host_at(CpuState {
        sp: 0xC000,
        ..at(0x0200)
    });
    host.load(0x0200, &[0xD9]);
    host.load(0xC000, &[0x34, 0x12]);
    assert_eq!(cpu.step(&mut host), 4);
    assert!(cpu.ime && !cpu.ei_pending && cpu.pc == 0x1234);
}

#[test]
fn halt_ime1_no_pending_halts_then_dispatches_on_wake() {
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 1,
        ime: true,
        ..at(0x0200)
    });
    host.load(0x0200, &[0x76, 0x00]);
    assert_eq!(cpu.step(&mut host), 2);
    assert!(cpu.halted && cpu.pc == 0x0201);
    assert_eq!(host.log, vec![("read", 0x0200, 1), ("read", 0x0201, 0)]);
    host.log.clear();
    assert_eq!(run(&mut cpu, &mut host, 3), vec![1, 1, 1]);
    assert_eq!(host.log, vec![("internal", 0x0000, 0); 3]);
    cpu.request_interrupt(0, false, false);
    assert_eq!(cpu.step(&mut host), 5);
    assert!(cpu.pc == 0x40 && !cpu.halted);
    assert_eq!((host.memory[0xC0FF], host.memory[0xC0FE]), (0x02, 0x01));
}

#[test]
fn halt_ime0_wakes_without_dispatch_and_the_halt_bug() {
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 1,
        ..at(0x0200)
    });
    host.load(0x0200, &[0x76, 0x3C]);
    cpu.step(&mut host);
    cpu.request_interrupt(0, false, false);
    host.log.clear();
    assert_eq!(cpu.step(&mut host), 1);
    assert_eq!(host.log, vec![("read", 0x0201, 1)]);
    assert_eq!((cpu.a, cpu.pc, cpu.if_), (1, 0x0202, 1));

    let (mut cpu, mut host) = host_at(CpuState {
        ie: 1,
        if_: 1,
        ..at(0x0200)
    });
    host.load(0x0200, &[0x76, 0x3C]);
    assert_eq!(cpu.step(&mut host), 1);
    assert!(!cpu.halted && cpu.halt_bug);
    cpu.step(&mut host);
    assert_eq!((cpu.pc, cpu.a, cpu.halt_bug), (0x0201, 1, false));
    cpu.step(&mut host);
    assert_eq!((cpu.pc, cpu.a), (0x0202, 2));
}

#[test]
fn halt_with_pending_and_ime_dispatches_from_the_halt() {
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 1,
        if_: 1,
        ei_pending: true,
        ..at(0x0200)
    });
    host.load(0x0200, &[0x76]);
    assert_eq!(cpu.step(&mut host), 1);
    assert!(!cpu.halted && cpu.halt_dispatch);
    host.log.clear();
    assert_eq!(cpu.step(&mut host), 5);
    assert_eq!(
        host.log,
        vec![
            ("read", 0x0201, 0),
            ("internal", 0x0201, -1),
            ("internal", 0xC100, -1),
            ("write", 0xC0FF, -1),
            ("write", 0xC0FE, 0),
        ]
    );
    assert_eq!((host.memory[0xC0FF], host.memory[0xC0FE]), (0x02, 0x00));
    assert!(!cpu.halt_dispatch && cpu.pc == 0x40);
}

#[test]
fn halt_decides_on_the_state_before_its_fetch_and_halt_sees() {
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 1,
        ..at(0x0200)
    });
    host.load(0x0200, &[0x76, 0x3C]);
    host.on_cycle = Some(Box::new(|n, cpu| {
        if n == 1 {
            cpu.request_interrupt(0, false, false)
        }
    }));
    assert_eq!(cpu.step(&mut host), 2);
    assert!(cpu.halted && !cpu.halt_bug);

    let (mut cpu, mut host) = host_at(CpuState {
        ie: 2,
        ime: true,
        ..at(0x0200)
    });
    host.load(0x0200, &[0x76, 0x00]);
    host.on_cycle = Some(Box::new(|n, cpu| {
        if n == 1 {
            cpu.request_interrupt(1, true, false)
        }
    }));
    assert_eq!(cpu.step(&mut host), 1);
    assert!(!cpu.halted && cpu.halt_dispatch);
}

#[test]
fn ei_halt_with_pending_returns_to_the_halt() {
    let (mut cpu, mut host) = host_at(CpuState {
        ie: 1,
        if_: 1,
        ..at(0x0200)
    });
    host.load(0x0200, &[0xFB, 0x76]);
    run(&mut cpu, &mut host, 2);
    assert!(!cpu.halted && !cpu.halt_bug && cpu.halt_dispatch);
    assert_eq!(cpu.step(&mut host), 5);
    assert_eq!((host.memory[0xC0FF], host.memory[0xC0FE]), (0x02, 0x01));
}

#[test]
fn stop_default_and_hook() {
    let (mut cpu, mut host) = host_at(at(0x0200));
    host.load(0x0200, &[0x10, 0x00, 0x3C]);
    assert_eq!(cpu.step(&mut host), 2);
    assert!(cpu.stopped && cpu.pc == 0x0202);
    assert_eq!(run(&mut cpu, &mut host, 3), vec![1, 1, 1]);
    cpu.request_interrupt(4, false, false);
    assert_eq!(cpu.step(&mut host), 1);
    assert!(!cpu.stopped);
    cpu.step(&mut host);
    assert_eq!(cpu.a, 1);

    for (result, cycles, pc, stopped, halted) in [
        (StopResult::new(StopAction::Stop), 2, 0x0202, true, false),
        (
            StopResult::with_operand(StopAction::Stop, false),
            1,
            0x0201,
            true,
            false,
        ),
        (StopResult::new(StopAction::Halt), 2, 0x0202, false, true),
        (
            StopResult::with_operand(StopAction::Nop, false),
            1,
            0x0201,
            false,
            false,
        ),
    ] {
        let (mut cpu, mut host) = host_at(CpuState {
            ie: 1,
            if_: 1,
            ..at(0x0200)
        });
        host.stop = Some(result);
        host.load(0x0200, &[0x10, 0x00]);
        assert_eq!(cpu.step(&mut host), cycles);
        assert_eq!(cpu.pc, pc);
        assert_eq!(host.seen, vec![true]);
        assert_eq!((cpu.stopped, cpu.halted), (stopped, halted));
    }
}

#[test]
fn the_core_owns_ffff() {
    let (mut cpu, mut host) = host_at(CpuState {
        a: 0x15,
        h: 0xFF,
        l: 0xFF,
        ..at(0x0200)
    });
    host.load(0x0200, &[0x77]);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = seen.clone();
    host.on_cycle = Some(Box::new(move |_, cpu| log.borrow_mut().push(cpu.ie)));
    cpu.step(&mut host);
    assert_eq!(seen.borrow()[1], 0x15);
    assert_eq!(host.memory[0xFFFF], 0x15);

    let (mut cpu, mut host) = host_at(CpuState {
        ie: 0xE5,
        h: 0xFF,
        l: 0xFF,
        ..at(0x0200)
    });
    host.load(0x0200, &[0x7E]);
    cpu.step(&mut host);
    assert_eq!(cpu.a, 0xE5);
    assert!(host.log.contains(&("read", 0xFFFF, 0)));
}

#[test]
fn pop_af_reset_and_state() {
    let (mut cpu, mut host) = host_at(CpuState {
        sp: 0xC000,
        ..at(0x0200)
    });
    host.load(0x0200, &[0xF1]);
    host.load(0xC000, &[0xFF, 0x12]);
    cpu.step(&mut host);
    assert_eq!(cpu.af(), 0x12F0);

    let (mut cpu, _) = host_at(CpuState {
        a: 1,
        f: 0xB0,
        ime: true,
        ei_pending: true,
        halted: true,
        ie: 0x1F,
        if_: 0x03,
        ..at(0x0200)
    });
    cpu.reset();
    let s = cpu.capture_state();
    assert_eq!(
        (s.pc, s.ie, s.ime, s.ei_pending, s.halted),
        (0, 0, false, false, false)
    );
    assert_eq!((s.a, s.f, s.if_), (1, 0xB0, 3));
    let original = CpuState {
        a: 9,
        f: 0x50,
        sp: 0xFFFE,
        pc: 0x0150,
        ime: true,
        locked: true,
        stop_if: 4,
        ..CpuState::default()
    };
    cpu.restore_state(&original);
    assert_eq!(cpu.capture_state(), original);
}

#[test]
#[should_panic(expected = "f bits 3..0")]
fn state_validation() {
    Sm83::new().restore_state(&CpuState {
        f: 0x01,
        ..CpuState::default()
    });
}
