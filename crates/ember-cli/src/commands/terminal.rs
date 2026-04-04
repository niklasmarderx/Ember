//! Terminal I/O helpers — echo suppression, stdin draining, line reading.

use anyhow::Result;
use std::io;

/// Read a single line from stdin.
pub fn read_line() -> Result<String> {
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input)
}

/// Suppress or restore terminal echo so keystrokes during AI output
/// don't appear on screen but remain in the stdin buffer.
#[cfg(unix)]
pub fn suppress_echo(suppress: bool) {
    // Use nix-less raw termios via std::os::unix
    use std::os::unix::io::AsRawFd;
    let fd = std::io::stdin().as_raw_fd();

    // We store/restore via a static to avoid unsafe global state issues.
    // The simpler approach: just flip the ECHO bit each time.
    unsafe {
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
        if libc::tcgetattr(fd, termios.as_mut_ptr()) == 0 {
            let mut t = termios.assume_init();
            if suppress {
                t.c_lflag &= !(libc::ECHO);
            } else {
                t.c_lflag |= libc::ECHO;
            }
            libc::tcsetattr(fd, libc::TCSANOW, &t);
        }
    }
}

#[cfg(not(unix))]
pub fn suppress_echo(_suppress: bool) {}

/// Drain any bytes sitting in the stdin buffer (non-blocking).
/// Returns them as a UTF-8 string, filtering out control characters.
pub fn drain_stdin() -> String {
    use std::io::Read;
    let mut buf = String::new();

    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = std::io::stdin().as_raw_fd();

        // Set non-blocking
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags >= 0 {
            unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };

            let mut raw = [0u8; 1024];
            if let Ok(n) = std::io::stdin().lock().read(&mut raw) {
                if let Ok(s) = std::str::from_utf8(&raw[..n]) {
                    buf.push_str(s);
                }
            }

            // Restore blocking mode
            unsafe { libc::fcntl(fd, libc::F_SETFL, flags) };
        }
    }

    // Filter: keep printable chars + space
    buf.retain(|c| c.is_alphanumeric() || c.is_ascii_punctuation() || c == ' ');
    buf
}
