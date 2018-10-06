#![recursion_limit = "1024"]

#[macro_use]
extern crate error_chain;

extern crate bit_field;
extern crate clap;
extern crate goblin;
extern crate pad;
extern crate tabwriter;
extern crate zmu_cortex_m;

use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use goblin::Object;
use pad::PadStr;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::time::Instant;
use tabwriter::TabWriter;

use zmu_cortex_m::core::instruction::Instruction;
use zmu_cortex_m::core::ThumbCode;
use zmu_cortex_m::semihosting::{SemihostingCommand, SemihostingResponse, SysExceptionReason};

use zmu_cortex_m::device::cortex_m::cortex_m0::cortex_m0_simulate;
use zmu_cortex_m::device::cortex_m::cortex_m0::cortex_m0_simulate_trace;

// We'll put our errors in an `errors` module, and other modules in
// this crate will `use errors::*;` to get access to everything
// `error_chain!` creates.
mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain! {}
}

use errors::*;

fn run_bin(
    code: &[u8],
    trace: bool,
    instructions: Option<u64>,
    option_trace_start: Option<u64>,
    symboltable: &HashMap<u32, &str>,
) {
    let _max_instructions = instructions.unwrap_or(0xffff_ffff_ffff_ffff);
    let start = Instant::now();

    let semihost_func = |semihost_cmd: &SemihostingCommand| -> SemihostingResponse {
        match semihost_cmd {
            &SemihostingCommand::SysOpen { .. } => SemihostingResponse::SysOpen { result: Ok(1) },
            &SemihostingCommand::SysClose { .. } => SemihostingResponse::SysClose { success: true },
            &SemihostingCommand::SysWrite { handle, ref data } => {
                if handle == 1 {
                    let text = &**data;
                    print!("{}", String::from_utf8_lossy(text));
                } else {}
                SemihostingResponse::SysWrite { result: Ok(0) }
            }
            &SemihostingCommand::SysClock { .. } => {
                let elapsed = start.elapsed();
                let in_cs = elapsed.as_secs() * 100 + elapsed.subsec_nanos() as u64 / 10_000_000;

                SemihostingResponse::SysClock {
                    result: Ok(in_cs as u32),
                }
            }
            &SemihostingCommand::SysException { ref reason } => {
                let mut stop = false;
                if reason == &SysExceptionReason::ADPStoppedApplicationExit {
                    stop = true;
                } else {
                    println!("semihosting exception!");
                }

                SemihostingResponse::SysException {
                    success: true,
                    stop: stop,
                }
            }
        }
    };

    let mut trace_stdout = TabWriter::new(io::stdout()).minwidth(16).padding(1);
    let trace_start = option_trace_start.unwrap_or(0);
    //    4803        ldr r0, =0x20010000 <__stack_end__>             0x000001A2    Reset_Handler    1
    let instruction_count = if trace {
        let tracefunc = |opcode: &ThumbCode, count: u64, pc: u32, instruction: &Instruction, r0_12: [u32; 13]| {
            if trace && count >= trace_start {
                let opcode_str = match *opcode {
                    ThumbCode::Thumb32 { opcode } => format!("{:08X}", opcode).with_exact_width(8),
                    ThumbCode::Thumb16 { half_word } => {
                        format!("{:04X}", half_word).with_exact_width(8)
                    }
                };

                let instruction_str = format!("{}", instruction).with_exact_width(32);
                let symbol = symboltable.get(&pc).unwrap_or(&"").with_exact_width(32);
                writeln!(
                    &mut trace_stdout,
                    "    {0:}    {1:} 0x{2:08X}    {3:}    {4:} r0:{5:08x} r1:{6:08x} r2:{7:08x} r3:{8:08x} r4:{9:08x} r5:{10:08x} r6:{11:08x} r7:{12:08x} r8:{13:08x} r9:{14:08x}",
                    opcode_str, instruction_str, pc, symbol, count, r0_12[0], r0_12[1], r0_12[2], r0_12[3], r0_12[4], r0_12[5], r0_12[6], r0_12[7], r0_12[8], r0_12[9],
                ).unwrap();
                let _ = trace_stdout.flush();
            }
        };
        cortex_m0_simulate_trace(code, tracefunc, semihost_func)
    } else {
        cortex_m0_simulate(code, semihost_func)
    };

    let end = Instant::now();

    let duration = end.duration_since(start);

    println!(
        "{:?}, {} instructions, {} instructions per sec",
        duration,
        instruction_count,
        instruction_count as f64
            / (duration.as_secs() as f64 + (duration.subsec_nanos() as f64 / 1000_000_000f64))
    );
}

fn run(args: &ArgMatches) -> Result<()> {
    match args.subcommand() {
        ("run", Some(run_matches)) => {
            let _device = run_matches.value_of("device").unwrap_or("cortex-m0");
            let filename = run_matches.value_of("EXECUTABLE").unwrap();
            let mut flash_mem = [0; 32768];
            let instructions = match run_matches.value_of("instructions") {
                Some(instr) => Some(instr.parse::<u64>().unwrap()),
                None => None,
            };
            let trace_start = match run_matches.value_of("trace_start") {
                Some(instr) => Some(instr.parse::<u64>().unwrap()),
                None => None,
            };

            let buffer = {
                let mut v = Vec::new();
                let mut f = File::open(&filename).chain_err(|| "unable to open file")?;
                f.read_to_end(&mut v).chain_err(|| "failed to read file")?;
                v
            };
            let mut symboltable = HashMap::new();
            let res = Object::parse(&buffer).unwrap();
            match res {
                Object::Elf(elf) => {
                    //println!("elf: {:#?}", &elf);
                    for ph in elf.program_headers {
                        if ph.p_type == goblin::elf::program_header::PT_LOAD {
                            /*println!(
                                "load: {} bytes from offset 0x{:x} to addr 0x{:x}",
                                ph.p_filesz, ph.p_offset, ph.p_paddr
                            );*/
                            if ph.p_filesz > 0 {
                                let dst_addr = ph.p_paddr as usize;
                                let dst_end_addr = (ph.p_paddr + ph.p_filesz) as usize;

                                let src_addr = ph.p_offset as usize;
                                let src_end_addr = (ph.p_offset + ph.p_filesz) as usize;
                                flash_mem[dst_addr..dst_end_addr]
                                    .copy_from_slice(&buffer[src_addr..src_end_addr]);
                            }
                        }
                    }

                    for sym in elf.syms {
                        if sym.st_type() != goblin::elf::sym::STT_FILE {
                            match elf.strtab.get(sym.st_name) {
                                Some(maybe_name) => {
                                    let name = maybe_name.unwrap_or("unknown");
                                    let mut count = 0;
                                    let mut pos = sym.st_value as u32;
                                    while count <= sym.st_size {
                                        // Align addresses to 2 byte alignment
                                        symboltable.insert(pos & 0xffff_fffe, name);
                                        pos += 2;
                                        count += 2;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {
                    panic!("unsupported file format");
                }
            }

            run_bin(
                &flash_mem,
                run_matches.is_present("trace"),
                instructions,
                trace_start,
                &symboltable,
            );
        }
        ("devices", Some(_)) => {
            println!("cortex-m0");
        }
        ("", None) => panic!("No sub command found"),
        _ => unreachable!(), // If all subcommands are defined above, anything else is unreachabe!()
    }

    Ok(())
}

fn main() {
    let args = App::new("zmu")
        .version("1.0")
        .about("a Low level emulator for microcontrollers")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(
            SubCommand::with_name("run")
                .about("Load and run <EXECUTABLE>")
                .arg(
                    Arg::with_name("device")
                        .short("d")
                        .long("device")
                        .help("Use specific device")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("trace")
                        .short("t")
                        .long("trace")
                        .help("Print instruction trace to stdout"),
                )
                .arg(
                    Arg::with_name("instructions")
                        .short("n")
                        .long("max_instructions")
                        .help("Max number of instructions to run")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("trace_start")
                        .long("trace_start")
                        .help("Instruction on which to start tracing")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("EXECUTABLE")
                        .index(1)
                        .help("Set executable to load")
                        .required(true),
                ),
        )
        .subcommand(SubCommand::with_name("devices").about("List available devices"))
        .get_matches();

    if let Err(ref e) = run(&args) {
        println!("error: {}", e);

        for e in e.iter().skip(1) {
            println!("caused by: {}", e);
        }

        if let Some(backtrace) = e.backtrace() {
            println!("backtrace: {:?}", backtrace);
        }

        ::std::process::exit(1);
    }
}
