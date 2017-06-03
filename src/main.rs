#![deny(warnings)]
#![feature(asm)]
#![feature(const_fn)]

extern crate orbclient;
extern crate orbfont;

#[cfg(not(target_os = "redox"))]
extern crate libc;

#[cfg(target_os = "redox")]
extern crate syscall;

use std::{cmp, env, io, str};
use std::io::Write;
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use before_exec::before_exec;
use console::Console;
use getpty::getpty;
use handle::handle;
use slave_stdio::slave_stdio;

mod before_exec;
mod console;
mod getpty;
mod handle;
mod slave_stdio;

fn main() {
    let mut args = env::args().skip(1);
    let shell = args.next().unwrap_or(env::var("SHELL").unwrap_or("sh".to_string()));

    let (display_width, display_height) = orbclient::get_display_size().expect("terminal: failed to get display size");
    let (columns, lines) = (cmp::min(1024, display_width * 4/5) / 8, cmp::min(768, display_height * 4/5) / 16);

    let (master_fd, tty_path) = getpty(columns, lines);
    let (slave_stdin, slave_stdout, slave_stderr) = slave_stdio(&tty_path).expect("terminal: failed to get slave stdio");

    let mut command = Command::new(&shell);
    for arg in args {
        command.arg(arg);
    }
    unsafe {
        command
        .stdin(Stdio::from_raw_fd(slave_stdin.into_raw_fd()))
        .stdout(Stdio::from_raw_fd(slave_stdout.into_raw_fd()))
        .stderr(Stdio::from_raw_fd(slave_stderr.into_raw_fd()))
        .env("COLUMNS", format!("{}", columns))
        .env("LINES", format!("{}", lines))
        .env("TERM", "xterm-256color")
        .env("TTY", tty_path)
        .before_exec(|| {
            before_exec()
        });
    }

    match command.spawn() {
        Ok(mut process) => {
            let mut console = Console::new(columns * 8, lines * 16);
            handle(&mut console, master_fd, &mut process);
        },
        Err(err) => {
            let term_stderr = io::stderr();
            let mut term_stderr = term_stderr.lock();
            let _ = writeln!(term_stderr, "terminal: failed to execute '{}': {:?}", shell, err);
        }
    }
}
