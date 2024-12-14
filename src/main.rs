use anyhow::Result;
use clap::Parser;
use nix::sys::ptrace;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult, Pid};
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
struct Opts {
    binary_path: PathBuf,
}

fn main() -> Result<()> {
    let opts = Opts::parse();

    match unsafe { fork()? } {
        ForkResult::Parent { child } => {
            debugger_loop(child)?;
        }
        ForkResult::Child => {
            ptrace::traceme()?;
            Command::new(opts.binary_path).exec();
        }
    }

    Ok(())
}

fn debugger_loop(child_pid: Pid) -> Result<()> {
    loop {
        match waitpid(child_pid, None)? {
            WaitStatus::Stopped(pid, signal) => {
                println!("Process {} stopped by signal {:?}", pid, signal);
                handle_debugger_command(child_pid)?;
            }
            WaitStatus::Exited(pid, status) => {
                println!("Process {} exited with status {}", pid, status);
                break;
            }
            _ => {}
        }
    }
    Ok(())
}

fn handle_debugger_command(child_pid: Pid) -> Result<()> {
    print!("(my-rust-debugger) ");
    io::stdout().flush()?;

    let mut command = String::new();
    io::stdin().read_line(&mut command)?;
    let command = command.trim();

    match command {
        "continue" | "c" => {
            ptrace::cont(child_pid, None)?;
        }
        "step" | "s" => {
            ptrace::step(child_pid, None)?;
        }
        cmd if cmd.starts_with("break") => {
            if let Some(address) = parse_breakpoint_command(cmd) {
                set_breakpoint(child_pid, address)?;
            } else {
                println!("Invalid breakpoint command. Usage: break <address>");
            }
        }
        _ => {
            println!("Unknown command: {}", command);
        }
    }
    Ok(())
}

fn parse_breakpoint_command(command: &str) -> Option<u64> {
    command
        .split_whitespace()
        .nth(1)
        .and_then(|addr| u64::from_str_radix(addr, 16).ok())
}

fn set_breakpoint(child_pid: Pid, addr: u64) -> Result<()> {
    // Read the original instruction at the address
    let original_data = ptrace::read(child_pid, addr as *mut _)?;

    // Replace the first byte with the INT3 instruction (0xCC)
    let breakpoint_data = (original_data & !0xFF) | 0xCC;
    ptrace::write(child_pid, addr as *mut _, breakpoint_data)?;

    println!("Breakpoint set at address 0x{:x}", addr);
    Ok(())
}
