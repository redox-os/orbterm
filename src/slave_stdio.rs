use libc::{O_CLOEXEC, O_RDONLY, O_WRONLY};
use std::ffi::CString;
use std::fs::File;
use std::io::{self, Result};
use std::os::unix::io::{FromRawFd, RawFd};

pub fn slave_stdio(tty_path: &str) -> Result<(File, File, File)> {
    let cvt = |res: i32| -> Result<i32> {
        if res < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(res)
        }
    };

    let tty_c = CString::new(tty_path).unwrap();
    let stdin = unsafe {
        File::from_raw_fd(cvt(libc::open(tty_c.as_ptr(), O_CLOEXEC | O_RDONLY))? as RawFd)
    };
    let stdout = unsafe {
        File::from_raw_fd(cvt(libc::open(tty_c.as_ptr(), O_CLOEXEC | O_WRONLY))? as RawFd)
    };
    let stderr = unsafe {
        File::from_raw_fd(cvt(libc::open(tty_c.as_ptr(), O_CLOEXEC | O_WRONLY))? as RawFd)
    };

    Ok((stdin, stdout, stderr))
}
