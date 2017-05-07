use std::io;

#[cfg(not(target_os="redox"))]
pub fn before_exec() -> io::Result<()> {
    use libc;

    unsafe {
        if libc::setsid() < 0 {
            panic!("setsid: {:?}", io::Error::last_os_error());
        }
        if libc::ioctl(0, libc::TIOCSCTTY, 1) < 0 {
            panic!("ioctl: {:?}", io::Error::last_os_error());
        }
    }

    Ok(())
}

#[cfg(target_os="redox")]
pub fn before_exec() -> io::Result<()> {
    Ok(())
}
