use std::os::unix::io::RawFd;

#[cfg(not(target_os = "redox"))]
pub fn getpty(columns: u32, lines: u32) -> (RawFd, String) {
    use std::ffi::CStr;
    use std::fs::OpenOptions;
    use std::io::{self, Error};
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::IntoRawFd;

    extern "C" {
        fn ptsname(fd: libc::c_int) -> *const libc::c_char;
        fn grantpt(fd: libc::c_int) -> libc::c_int;
        fn unlockpt(fd: libc::c_int) -> libc::c_int;
    }

    let master_fd = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open("/dev/ptmx")
        .unwrap()
        .into_raw_fd();
    unsafe {
        if grantpt(master_fd) < 0 {
            panic!("grantpt: {:?}", Error::last_os_error());
        }
        if unlockpt(master_fd) < 0 {
            panic!("unlockpt: {:?}", Error::last_os_error());
        }
    }

    unsafe {
        let size = libc::winsize {
            ws_row: lines as libc::c_ushort,
            ws_col: columns as libc::c_ushort,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        if libc::ioctl(master_fd, libc::TIOCSWINSZ, &size as *const libc::winsize) < 0 {
            panic!("ioctl: {:?}", io::Error::last_os_error());
        }
    }

    let tty_path = unsafe {
        CStr::from_ptr(ptsname(master_fd))
            .to_string_lossy()
            .into_owned()
    };
    (master_fd, tty_path)
}

#[cfg(target_os = "redox")]
pub fn getpty(columns: u32, lines: u32) -> (RawFd, String) {
    use libredox::{call as redox, flag};
    let master = redox::open(
        "/scheme/pty",
        flag::O_CLOEXEC | flag::O_RDWR | flag::O_CREAT | flag::O_NONBLOCK,
        0,
    )
    .unwrap();

    if let Ok(winsize_fd) = redox::dup(master, b"winsize") {
        let _ = redox::write(
            winsize_fd,
            &redox_termios::Winsize {
                ws_row: lines as u16,
                ws_col: columns as u16,
            },
        );
        let _ = redox::close(winsize_fd);
    }

    let mut buf: [u8; 4096] = [0; 4096];
    let count = redox::fpath(master, &mut buf).unwrap();
    (master as RawFd, unsafe {
        String::from_utf8_unchecked(Vec::from(&buf[..count]))
    })
}
