use std::io;

pub fn before_exec() -> io::Result<()> {
    unsafe {
        if libc::setsid() < 0 {
            panic!("setsid: {:?}", io::Error::last_os_error());
        }
        if libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
            panic!("ioctl: {:?}", io::Error::last_os_error());
        }
    }

    Ok(())
}
