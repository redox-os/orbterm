use orbclient::event::EventOption;
use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::io::{FromRawFd, RawFd};
use std::process::Child;

use console::Console;

#[cfg(target_os = "redox")]
pub fn handle(console: &mut Console, master_fd: RawFd, process: &mut Child) {
    extern crate syscall;

    use std::os::unix::io::AsRawFd;

    let mut event_file = File::open("event:").expect("terminal: failed to open event file");

    let window_fd = console.window.as_raw_fd();
    syscall::fevent(window_fd, syscall::flag::EVENT_READ).expect("terminal: failed to fevent console window");

    let mut master = unsafe { File::from_raw_fd(master_fd) };
    syscall::fevent(master_fd, syscall::flag::EVENT_READ).expect("terminal: failed to fevent master PTY");

    let mut handle_event = |event_id: usize, event_count: usize| -> bool {
        if event_id == window_fd {
            for event in console.window.events() {
                let event_option = event.to_option();

                console.input(event_option);

                match event_option {
                    EventOption::Quit(_) => return false,
                    EventOption::Resize(_) => {
                        //TODO: Send resize to PTY
                    },
                    _ => ()
                }
            }
        } else if event_id == master_fd {
            let mut packet = [0; 4096];
            let count = master.read(&mut packet).expect("terminal: failed to read master PTY");
            if count == 0 {
                if event_count == 0 {
                    return false;
                }
            } else {
                console.write(&packet[1..count], true).expect("terminal: failed to write to console");

                if packet[0] & 1 == 1 {
                    console.redraw();
                }
            }
        } else {
            println!("Unknown event {}", event_id);
        }

        if ! console.input.is_empty()  {
            if let Err(err) = master.write(&console.input) {
                let term_stderr = io::stderr();
                let mut term_stderr = term_stderr.lock();
                let _ = writeln!(term_stderr, "terminal: failed to write stdin: {:?}", err);
                return false;
            }
            let _ = master.flush();
            console.input.clear();
        }

        true
    };

    handle_event(window_fd, 0);
    handle_event(master_fd, 0);

    'events: loop {
        let mut sys_event = syscall::Event::default();
        event_file.read(&mut sys_event).expect("terminal: failed to read event file");
        if ! handle_event(sys_event.id, sys_event.data) {
            break 'events;
        }

        match process.try_wait() {
            Ok(status) => match status {
                Some(_code) => break 'events,
                None => ()
            },
            Err(err) => match err.kind() {
                ErrorKind::WouldBlock => (),
                _ => panic!("terminal: failed to wait on child: {:?}", err)
            }
        }
    }

    let _ = process.kill();
    process.wait().expect("terminal: failed to wait on shell");
}

#[cfg(not(target_os = "redox"))]
pub fn handle(console: &mut Console, master_fd: RawFd, process: &mut Child) {
    use libc;
    use std::thread;
    use std::time::Duration;

    console.console.raw_mode = true;

    let mut master = unsafe { File::from_raw_fd(master_fd) };

    'events: loop {
        for event in console.window.events() {
            let event_option = event.to_option();

            console.input(event_option);

            match event_option {
                EventOption::Quit(_) => break 'events,
                EventOption::Resize(_) => unsafe {
                    let size = libc::winsize {
                        ws_row: console.console.h as libc::c_ushort,
                        ws_col: console.console.w as libc::c_ushort,
                        ws_xpixel: 0,
                        ws_ypixel: 0
                    };
                    if libc::ioctl(master_fd, libc::TIOCSWINSZ, &size as *const libc::winsize) < 0 {
                        panic!("ioctl: {:?}", io::Error::last_os_error());
                    }
                },
                _ => ()
            }
        }

        let mut packet = [0; 4096];
        match master.read(&mut packet) {
            Ok(0) => {
                break 'events;
            },
            Ok(count) => {
                console.write(&packet[..count], true).expect("terminal: failed to write to console");
                console.redraw();
            },
            Err(err) => match err.kind() {
                ErrorKind::WouldBlock => (),
                _ => panic!("terminal: failed to read master PTY: {:?}", err)
            }
        }

        if ! console.input.is_empty()  {
            if let Err(err) = master.write(&console.input) {
                let term_stderr = io::stderr();
                let mut term_stderr = term_stderr.lock();
                let _ = writeln!(term_stderr, "terminal: failed to write stdin: {:?}", err);
                break 'events;
            }
            let _ = master.flush();
            console.input.clear();
        }

        match process.try_wait() {
            Ok(status) => match status {
                Some(code) => {
                    break 'events;
                },
                None => ()
            },
            Err(err) => match err.kind() {
                ErrorKind::WouldBlock => (),
                _ => panic!("terminal: failed to wait on child: {:?}", err)
            }
        }

        thread::sleep(Duration::new(0, 10));
    }

    let _ = process.kill();
    process.wait().expect("terminal: failed to wait on shell");
}
