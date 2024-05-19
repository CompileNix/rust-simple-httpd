use std::fmt;
use std::io::IsTerminal;
use std::num::NonZero;

#[cfg(feature = "humantime")]
use time::util::local_offset::Soundness;

use crate::config::Config;

/// Macro to define an enum and automatically implement the following functions:
/// - `from_usize`
/// - `from_str`
/// - `iter`
#[macro_export]
macro_rules! enum_with_helpers {
    ($vis:vis enum $name:ident { $($variant:ident),+ $(,)? } default: $default_variant:ident) => {
        #[repr(usize)]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
        $vis enum $name {
            $($variant),+
        }

        impl Default for $name {
            fn default() -> Self {
                $name::$default_variant
            }
        }

        use std::str::FromStr;
        pub struct ParseEnumError;
        impl FromStr for $name {
            type Err = ParseEnumError;

            /// Function to convert a &str to an enum variant
            fn from_str(s: &str) -> Result<Level, ParseEnumError> {
                match s.to_lowercase().as_str() {
                    $(
                        _s if _s == stringify!($variant).to_lowercase() => Ok($name::$variant),
                    )+
                    _ => Err(ParseEnumError),
                }
            }
        }

        impl $name {
            /// Function to convert a usize to an enum variant
            #[must_use]
            pub fn from_usize(u: usize) -> Option<Self> {
                match u {
                    $(x if x == $name::$variant as usize => Some($name::$variant),)+
                    _ => None,
                }
            }

            /// Function to iterate over all enum variants
            #[allow(unused)]
            pub fn iter() -> impl Iterator<Item = Self> {
                static VARIANTS: &[$name] = &[$($name::$variant),+];
                VARIANTS.iter().copied()
            }
        }
    };
}

#[macro_export]
#[cfg(not(feature = "color"))]
macro_rules! struct_with_colorized_display_impl {
    (
        $vis_struct:vis struct $name:ident {
            $(
                $vis_field:vis $field:ident: $type:ty,
            )*
        }
    ) => {
        #[derive(Clone, Debug, Hash, Default)]
        $vis_struct struct $name {
            $(
                $vis_field $field: $type
            ),+
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                let value = format!("{self:?}");
                util::format_with_options(&value, f)
            }
        }
    };
}

#[macro_export]
#[cfg(feature = "color")]
macro_rules! struct_with_colorized_display_impl {
    (
        $vis_struct:vis struct $name:ident {
            $(
                $vis_field:vis $field:ident: $type:ty,
            )*
        }
    ) => {
        #[derive(Clone, Debug, Hash, Default)]
        $vis_struct struct $name {
            $(
                $vis_field $field: $type
            ),+
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                let mut output = String::new();

                if $crate::util::is_terminal_attached() {
                    output += &stringify!($name).to_string().green().text;
                    output += " {";

                    let mut formatted_field_value: String;
                    $(
                        formatted_field_value = format!("{:?}", self.$field);
                        output += &*format!(" {}: {},", stringify!($field).to_string().blue().text, formatted_field_value.yellow().text);
                    )*
                    // remove trailing ","
                    output.pop();

                    output += " }";
                } else {
                    output += &*format!("{self:?}");
                };

                util::format_with_options(&output, f)
            }
        }
    };
}

pub fn is_terminal_attached() -> bool {
    let is_tty_stdout = std::io::stdout().is_terminal();
    let is_tty_stderr = std::io::stderr().is_terminal();

    is_tty_stdout && is_tty_stderr
}

/// A terminal is not attached, disable ANSI colored output
pub fn is_colored_output_avail(config: &Config) -> bool {
    if config.colored_output_forced {
        return true;
    }

    config.colored_output && is_terminal_attached()
}

#[cfg(feature = "color")]
#[cfg(feature = "log-trace")]
fn write_formatted_eol_byte(byte: u8, config: &Config) -> String {
    use crate::color::Color;
    use crate::color::Colorize;
    let eol_color = Color::Yellow;

    match byte {
        b'\r' => {
            let replacement = String::from(r"\r");
            if is_colored_output_avail(config) {
                replacement.colorize(eol_color).text.to_string()
            } else {
                replacement.to_string()
            }
        }
        b'\n' => {
            let replacement = String::from(r"\n");
            if is_colored_output_avail(config) {
                replacement.colorize(eol_color).text.to_string()
            } else {
                replacement.to_string()
            }
        }
        _ => {
            format!("{byte:02x}")
        }
    }
}

#[cfg(not(feature = "color"))]
#[cfg(feature = "log-trace")]
fn write_formatted_eol_byte(byte: u8, config: &Config) -> String {
    // config is only used when color feature is enabled
    let _ = config;

    match byte {
        b'\r' => "\\r".to_string(),
        b'\n' => "\\n".to_string(),
        _ => {
            format!("{byte:02x}")
        }
    }
}

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

/// Counts the number of digits in a `usize` value by converting it to a string.
///
/// This function takes a `usize` value, converts it to its string representation,
/// and then counts the number of characters in the string. This count represents
/// the number of digits in the original `usize` value.
///
/// # Examples
///
/// Basic usage:
///
/// ```
/// let number: usize = 12345;
/// let digit_count = num_digits(number);
/// assert_eq!(digit_count, 5);
/// ```
///
/// Edge case with zero:
///
/// ```
/// let number: usize = 0;
/// let digit_count = num_digits(number);
/// assert_eq!(digit_count, 1);
/// ```
///
/// # Parameters
///
/// - `n`: The `usize` value for which the number of digits is to be counted.
///
/// # Returns
///
/// - A `usize` representing the number of digits in `n`.
///
/// # Panics
///
/// This function does not panic.
///
/// # Safety
///
/// This function involves no unsafe operations and is safe to call.
pub fn num_digits(n: usize) -> usize {
    // slow but reliable
    n.to_string().chars().count()
}

pub fn format_log_message_prefix(time: &str, level: &str, contains_ansi_color: bool) -> String {
    if contains_ansi_color {
        format!("[{time} {level:<14}]: ")
    } else {
        format!("[{time} {level:<5}]: ")
    }
}

/// Safety notice: <https://docs.rs/time/0.3.34/time/util/local_offset/fn.set_soundness.html#safety>
#[cfg(feature = "humantime")]
pub fn new_time_string() -> String {
    unsafe {
        time::util::local_offset::set_soundness(Soundness::Unsound);
    }
    let now = time::OffsetDateTime::now_local().unwrap_or(time::OffsetDateTime::now_utc());
    let time = now
        .format(
            &time::format_description::parse_borrowed::<2>(
                "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second]",
            )
            .expect("Could not format current datetime to string for log message"),
        )
        .unwrap_or_default();

    time
}

#[cfg(not(feature = "humantime"))]
#[allow(clippy::let_and_return)]
pub fn new_time_string() -> String {
    let now = time::OffsetDateTime::now_utc();
    let time = now
        .format(&time::format_description::well_known::Iso8601::DEFAULT)
        .unwrap_or_default();

    time
}

#[cfg(feature = "color")]
pub fn log_level_to_string_colorized(level: crate::log::Level) -> crate::color::ColorizedText {
    use crate::color::Colorize;

    let level_text = level.to_string();
    match level {
        crate::log::Level::Error => level_text.red(),
        crate::log::Level::Warn => level_text.yellow(),
        crate::log::Level::Info => level_text.green(),
        crate::log::Level::Verb => level_text.magenta(),
        crate::log::Level::Debug => level_text.blue(),
        crate::log::Level::Trace => level_text.cyan(),
    }
}

#[cfg(feature = "log-trace")]
pub fn highlighted_hex_vec(vec: &[u8], index_offset: usize, config: &Config) -> String {
    let mut output = String::with_capacity(250); // this size is rather small but also very rarely too small
    let digits = num_digits(index_offset + vec.len());

    let format_log_message_prefix_length: usize;
    #[cfg(feature = "color")]
    {
        let colorized_text = log_level_to_string_colorized(crate::log::Level::Trace);
        let format_log_message_prefix = format_log_message_prefix(
            &new_time_string(),
            &colorized_text.text,
            is_colored_output_avail(config),
        );
        format_log_message_prefix_length =
            format_log_message_prefix.len() - colorized_text.color_code_length;
    }
    #[cfg(not(feature = "color"))]
    {
        let text = crate::log::Level::Trace.to_string();
        let format_log_message_prefix = format_log_message_prefix(&new_time_string(), &text, false);
        format_log_message_prefix_length = format_log_message_prefix.len();
    }

    for (index, byte) in vec.iter().enumerate() {
        if index != 0 {
            output.push(' ');
        }

        if index % 8 == 0 {
            output.push('\n');
            output += &*" ".repeat(format_log_message_prefix_length);
            let current_index = index + index_offset;
            output.push_str(&format!("{current_index:digits$} = "));
        }

        output.push_str(&write_formatted_eol_byte(*byte, config));
    }

    output
}

pub fn available_parallelism_capped_at(max: usize) -> usize {
    let one = NonZero::new(1).expect("Somehow 1 is equal to 0 🤷");
    let avail = std::thread::available_parallelism().unwrap_or(one).get();

    match max {
        0 => avail,
        max if max < avail => max,
        _ => avail,
    }
}
