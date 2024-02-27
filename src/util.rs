use crate::config::Config;
use atty::Stream;
use std::fmt;

pub fn format_with_options(value: &impl fmt::Display, f: &mut fmt::Formatter) -> fmt::Result {
    if let Some(width) = f.width() {
        let alignment = f.align().unwrap_or(fmt::Alignment::Left);

        // Format output according to the width and alignment
        let formatted_value = match alignment {
            fmt::Alignment::Left => format!("{value:<width$}"),
            fmt::Alignment::Right => format!("{value:>width$}"),
            fmt::Alignment::Center => format!("{value:^width$}"),
        };

        // Write the formatted string to the formatter
        write!(f, "{formatted_value}")
    } else {
        // If no width is set, just write the value
        write!(f, "{value}")
    }
}

/// A terminal is not attached, disable ANSI colored output
pub fn enable_terminal_colors(config: Config) -> bool {
    if config.colored_output_forced {
        return true;
    }

    let colored_output = config.colored_output;
    // println!("colored_output: {colored_output}");
    let is_tty_stdout = atty::is(Stream::Stdout);
    // println!("is_tty_stdout: {is_tty_stdout}");
    let is_tty_stderr = atty::is(Stream::Stderr);
    // println!("is_tty_stderr: {is_tty_stderr}");
    let result = colored_output && is_tty_stdout && is_tty_stderr;
    // println!("result: {result}");

    #[allow(clippy::let_and_return)]
    result
}

#[cfg(feature = "color")]
fn write_formatted_eol_byte(byte: u8, prefix: &str, config: Config) -> String {
    use crate::color::Color;
    use crate::color::Colorize;
    let eol_color = Color::Yellow;
    let enable_terminal_colors = enable_terminal_colors(config);

    match byte {
        b'\r' => {
            let replacement = r"\r";
            if enable_terminal_colors {
                format!("{prefix}{}", replacement.colorize(eol_color))
            } else {
                format!("{prefix}{replacement}")
            }
        }
        b'\n' => {
            let replacement = r"\n";
            if enable_terminal_colors {
                format!("{prefix}{}", replacement.colorize(eol_color))
            } else {
                format!("{prefix}{replacement}")
            }
        }
        _ => {
            format!("{prefix}{byte:02x}")
        }
    }
}

#[cfg(not(feature = "color"))]
fn write_formatted_eol_byte(byte: u8, prefix: &str, config: Config) -> String {
    // config is only used when color feature is enabled
    let _ = config;

    match byte {
        b'\r' => {
            format!("{prefix}\\r")
        }
        b'\n' => {
            format!("{prefix}\\n")
        }
        _ => {
            format!("{prefix}{byte:02x}")
        }
    }
}

#[cfg(feature = "log-trace")]
pub fn highlighted_hex_vec(vec: &[u8], config: Config) -> String {
    let mut output = String::from('[');

    // Iterate over each element in the Vec<u8>, formatting it
    let mut iter = vec.iter();
    if let Some(byte) = iter.next() {
        output.push_str(&write_formatted_eol_byte(*byte, "", config));
    }
    for byte in iter {
        output.push_str(&write_formatted_eol_byte(*byte, ", ", config));
    }

    output.push(']');
    output
}
