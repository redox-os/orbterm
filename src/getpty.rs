use std::os::unix::io::RawFd;

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
