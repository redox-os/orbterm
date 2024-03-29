use orbclient::event::EventOption;
use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::io::{FromRawFd, RawFd};
use std::process::Child;

use console::Console;

#[cfg(target_os = "redox")]
pub fn handle(console: &mut Console, master_fd: RawFd, process: &mut Child) {
    use std::os::unix::io::AsRawFd;

    use event::{EventFlags, EventQueue};
    use libredox::call as redox;

    event::user_data! {
        enum EventSource {
            Window,
            Master,
        }
    };

    let event_queue = EventQueue::<EventSource>::new().expect("terminal: failed to open event file");

    let window_fd = console.window.as_raw_fd();
    event_queue.subscribe(window_fd as usize, EventSource::Window, EventFlags::READ)
        .expect("terminal: failed to fevent console window");

    let mut master = unsafe { File::from_raw_fd(master_fd) };
    event_queue.subscribe(master_fd as usize, EventSource::Master, EventFlags::READ)
        .expect("terminal: failed to fevent master PTY");

    let mut handle_event = |event_source: EventSource| -> bool {
        match event_source {
            EventSource::Window => for event in console.window.events() {
                let event_option = event.to_option();

                let console_w = console.ransid.state.w;
                let console_h = console.ransid.state.h;

                console.input(event_option);

                if let EventOption::Quit(_) = event_option {
                    return false;
                }

                if console_w != console.ransid.state.w || console_h != console.ransid.state.h {
                    if let Ok(winsize_fd) = redox::dup(master_fd as usize, b"winsize") {
                        let _ = redox::write(
                            winsize_fd,
                            &redox_termios::Winsize {
                                ws_row: console.ransid.state.h as u16,
                                ws_col: console.ransid.state.w as u16,
                            },
                        );
                        let _ = redox::close(winsize_fd);
                    }
                }
            }
            EventSource::Master => {
                let mut packet = [0; 4096];
                loop {
                    let count = match master.read(&mut packet) {
                        Ok(0) => return false,
                        Ok(count) => count,
                        Err(ref err) if err.kind() == ErrorKind::WouldBlock => break,
                        Err(_) => panic!("terminal: failed to read master PTY"),
                    };
                    console
                        .write(&packet[1..count], true)
                        .expect("terminal: failed to write to console");

                    if packet[0] & 1 == 1 {
                        console.redraw();
                    }
                }
            }
        }

        if !console.input.is_empty() {
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

    handle_event(EventSource::Window);
    handle_event(EventSource::Master);

    'events: for event_res in event_queue {
        let event = event_res.expect("terminal: failed to read event queue");

        if !handle_event(event.user_data) {
            break 'events;
        }

        match process.try_wait() {
            Ok(status) => match status {
                Some(_code) => break 'events,
                None => (),
            },
            Err(err) => match err.kind() {
                ErrorKind::WouldBlock => (),
                _ => panic!("terminal: failed to wait on child: {:?}", err),
            },
        }
    }

    let _ = process.kill();
    process.wait().expect("terminal: failed to wait on shell");
}

#[cfg(not(target_os = "redox"))]
pub fn handle(console: &mut Console, master_fd: RawFd, process: &mut Child) {
    use std::thread;
    use std::time::Duration;

    let mut master = unsafe { File::from_raw_fd(master_fd) };

    'events: loop {
        for event in console.window.events() {
            let event_option = event.to_option();

            let console_w = console.ransid.state.w;
            let console_h = console.ransid.state.h;

            console.input(event_option);

            if let EventOption::Quit(_) = event_option {
                break 'events;
            }

            if console_w != console.ransid.state.w || console_h != console.ransid.state.h {
                unsafe {
                    let size = libc::winsize {
                        ws_row: console.ransid.state.h as libc::c_ushort,
                        ws_col: console.ransid.state.w as libc::c_ushort,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
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
            }
            Ok(count) => {
                console
                    .write(&packet[..count], true)
                    .expect("terminal: failed to write to console");
                console.redraw();
            }
            Err(err) => match err.kind() {
                ErrorKind::WouldBlock => (),
                _ => panic!("terminal: failed to read master PTY: {:?}", err),
            },
        }

        if !console.input.is_empty() {
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
                }
                None => (),
            },
            Err(err) => match err.kind() {
                ErrorKind::WouldBlock => (),
                _ => panic!("terminal: failed to wait on child: {:?}", err),
            },
        }

        thread::sleep(Duration::new(0, 10));
    }

    let _ = process.kill();
    process.wait().expect("terminal: failed to wait on shell");
}
