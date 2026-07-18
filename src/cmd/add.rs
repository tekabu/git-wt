use std::io::Write;

/// Print a prompt to stderr and read a yes/no answer from stdin. Requires the
/// user to type and press Enter; empty or anything but y/yes is No. EOF / no
/// tty is No.
pub(crate) fn confirm(prompt: &str) -> Result<bool, String> {
    eprint!("{prompt}");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    let n = std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(false); // EOF / no tty -> treat as No
    }
    let a = line.trim().to_ascii_lowercase();
    Ok(a == "y" || a == "yes")
}
