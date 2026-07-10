const LOG_COLOUR: u8 = 2;
const INFO_COLOUR: u8 = 0;
const WARNING_COLOUR: u8 = 33;
const ERROR_COLOUR: u8 = 31;

pub fn format_msg(colour: u8, prefix: bool, msg: &str) -> String {
    format!("\x1b[{colour}m{}{msg}\x1b[0m", if prefix { ":: " } else { "" })
}

#[allow(dead_code)]
pub fn log(msg: &str) { println!("{}", format_msg(LOG_COLOUR, true, msg)); }
#[allow(dead_code)]
pub fn info(msg: &str) { println!("{}", format_msg(INFO_COLOUR, true, msg)); }
pub fn warn(msg: &str) { println!("{}", format_msg(WARNING_COLOUR, true, &format!("Warning: {msg}"))); }
#[allow(dead_code)]
pub fn error(msg: &str) { eprintln!("{}", format_msg(ERROR_COLOUR, true, &format!("Error: {msg}"))); }

#[allow(dead_code)]
pub fn fatal(msg: &str) -> ! {
    eprintln!("{}", format_msg(ERROR_COLOUR, true, &format!("Fatal: {msg}")));
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_matches_python() {
        assert_eq!(format_msg(33, true, "Warning: hi"), "\x1b[33m:: Warning: hi\x1b[0m");
        assert_eq!(format_msg(31, false, "Error: x"), "\x1b[31mError: x\x1b[0m");
    }
}
