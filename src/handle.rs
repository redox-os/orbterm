use orbclient::event::EventOption;
use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::io::{FromRawFd, RawFd};
use std::process::Child;

use console::Console;

#[cfg(target_os = "redox")]
pub fn handle(console: &mut Console, master_fd: RawFd, process: &mut Child) {
    use redox_termios;
    use syscall;

    use std::os::unix::io::AsRawFd;

    let mut event_file = File::open("event:").expect("terminal: failed to open event file");

    let window_fd = console.window.as_raw_fd();
    event_file.write(&syscall::data::Event {
        id: window_fd,
        flags: syscall::flag::EVENT_READ,
        data: 0
    }).expect("terminal: failed to fevent console window");

    let mut master = unsafe { File::from_raw_fd(master_fd) };
    event_file.write(&syscall::data::Event {
        id: master_fd,
        flags: syscall::flag::EVENT_READ,
        data: 0
    }).expect("terminal: failed to fevent master PTY");

    let mut handle_event = |event_id: usize| -> bool {
        if event_id == window_fd {
            for event in console.window.events() {
                let event_option = event.to_option();

                let console_w = console.console.state.w;
                let console_h = console.console.state.h;

                console.input(event_option);

                if let EventOption::Quit(_) = event_option {
                    return false;
                }

                if console_w != console.console.state.w || console_h != console.console.state.h {
                    if let Ok(winsize_fd) = syscall::dup(master_fd, b"winsize") {
                        let _ = syscall::write(winsize_fd, &redox_termios::Winsize {
                            ws_row: console.console.state.h as u16,
                            ws_col: console.console.state.w as u16
                        });
                        let _ = syscall::close(winsize_fd);
                    }
                }
            }
        } else if event_id == master_fd {
            let mut packet = [0; 4096];
            loop {
                let count = match master.read(&mut packet) {
                    Ok(0) => return false,
                    Ok(count) => count,
                    Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => panic!("terminal: failed to read master PTY")
                };
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

    handle_event(window_fd);
    handle_event(master_fd);

    'events: loop {
        let mut sys_event = syscall::Event::default();
        event_file.read(&mut sys_event).expect("terminal: failed to read event file");
        if ! handle_event(sys_event.id) {
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

    let mut master = unsafe { File::from_raw_fd(master_fd) };

    'events: loop {
        for event in console.window.events() {
            let event_option = event.to_option();

            let console_w = console.console.state.w;
            let console_h = console.console.state.h;

            console.input(event_option);

            if let EventOption::Quit(_) = event_option {
                break 'events;
            }

            if console_w != console.console.state.w || console_h != console.console.state.h {
                unsafe {
                    let size = libc::winsize {
                        ws_row: console.console.state.h as libc::c_ushort,
                        ws_col: console.console.state.w as libc::c_ushort,
                        ws_xpixel: 0,
                        ws_ypixel: 0
                    };
                    if libc::ioctl(master_fd, libc::TIOCSWINSZ, &size as *const libc::winsize) < 0 {
                        panic!("ioctl: {:?}", io::Error::last_os_error());
                    }
                }
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
                Some(_code) => {
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
