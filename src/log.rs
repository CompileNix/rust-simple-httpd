// vim: sw=4 et filetype=rust

use std::fmt;

use crate::config::Config;
use crate::util;
#[cfg(feature = "humantime")]
use time::util::local_offset::Soundness;

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

        impl $name {
            /// Function to convert a usize to an enum variant
            pub fn from_usize(u: usize) -> Option<Self> {
                match u {
                    $(x if x == $name::$variant as usize => Some($name::$variant),)+
                    _ => None,
                }
            }

            /// Function to convert a &str to an enum variant
            pub fn from_str(s: &str) -> Option<Self> {
                match s.to_lowercase().as_str() {
                    $(
                        _s if _s == stringify!($variant).to_lowercase() => Some($name::$variant),
                    )+
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

enum_with_helpers! {
    pub enum Level {
        Error,
        Warn,
        Info,
        Verb,
        Debug,
        Trace,
    } default: Warn
}

impl TryFrom<usize> for Level {
    type Error = &'static str;

    fn try_from(value: usize) -> Result<Self, <Level as TryFrom<usize>>::Error> {
        if let Some(val) = Self::from_usize(value) {
            Ok(val)
        } else {
            Err("Unknown log level provided")
        }
    }
}

impl TryFrom<&str> for Level {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, <Level as TryFrom<&str>>::Error> {
        if let Some(val) = Self::from_str(value) {
            Ok(val)
        } else {
            Err("Unknown log level provided")
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let value = format!("{self:?}");
        util::format_with_options(&value, f)
    }
}

pub struct Log {
    config: Config,
    message: String,
    level: Level,
    time: String,
}

impl Log {
    pub fn new(config: Config, text: &str, level: Level) -> Log {
        Log {
            level,
            message: text.to_string(),
            config,
            time: new_time_string(),
        }
    }
}

#[cfg(feature = "color")]
impl fmt::Display for Log {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.config.colored_output {
            use crate::color::Colorize;

            let level_text: &str = &format!("{}", self.level);
            let level = match self.level {
                Level::Error => level_text.red(),
                Level::Warn => level_text.yellow(),
                Level::Info => level_text.green(),
                Level::Verb => level_text.magenta(),
                Level::Debug => level_text.blue(),
                Level::Trace => level_text.cyan(),
            };

            let value = format_log_message(&self.time.clone(), &level, &self.message.clone(), true);
            util::format_with_options(&value, f)
        } else {
            let value = format_log_message(
                &self.time.clone(),
                &self.level.to_string(),
                &self.message.clone(),
                false,
            );
            util::format_with_options(&value, f)
        }
    }
}

#[cfg(not(feature = "color"))]
impl fmt::Display for Log {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // config is only used when color feature is enabled
        let _ = self.config;

        let value = format_log_message(
            &self.time.clone(),
            &self.level.to_string(),
            &self.message.clone(),
            false,
        );
        util::format_with_options(&value, f)
    }
}

pub fn format_log_message(
    time: &str,
    level: &str,
    message: &str,
    contains_ansi_color: bool,
) -> String {
    if contains_ansi_color {
        format!("[{time} {level:<14}]: {message}")
    } else {
        format!("[{time} {level:<5}]: {message}")
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

fn detect_tty_and_update_config_for(config: Config) -> Config {
    let mut config = config;

    // A terminal is not attached, disable ANSI colored output
    if !util::enable_terminal_colors(config) {
        config.colored_output = false;
    }

    config
}

#[allow(dead_code)]
pub fn error(config: Config, text: &str) {
    let config = detect_tty_and_update_config_for(config);
    let formatted_message = format!("{}", Log::new(config, text, Level::Error));
    eprintln!("{formatted_message}");
}

#[allow(dead_code)]
pub fn warn(config: Config, text: &str) {
    let config = detect_tty_and_update_config_for(config);
    let formatted_message = format!("{}", Log::new(config, text, Level::Warn));
    eprintln!("{formatted_message}");
}

#[allow(dead_code)]
pub fn info(config: Config, text: &str) {
    let config = detect_tty_and_update_config_for(config);
    let formatted_message = format!("{}", Log::new(config, text, Level::Info));
    println!("{formatted_message}");
}

#[allow(dead_code)]
pub fn verb(config: Config, text: &str) {
    let config = detect_tty_and_update_config_for(config);
    let formatted_message = format!("{}", Log::new(config, text, Level::Verb));
    eprintln!("{formatted_message}");
}

#[allow(dead_code)]
pub fn debug(config: Config, text: &str) {
    let config = detect_tty_and_update_config_for(config);
    let formatted_message = format!("{}", Log::new(config, text, Level::Debug));
    eprintln!("{formatted_message}");
}

#[allow(dead_code)]
pub fn trace(config: Config, text: &str) {
    let config = detect_tty_and_update_config_for(config);
    let formatted_message = format!("{}", Log::new(config, text, Level::Trace));
    eprintln!("{formatted_message}");
}

#[macro_export]
macro_rules! init {
    ($($arg:tt)*) => {{
        #[cfg(feature = "log-trace")]
        {
            let text = &std::fmt::format(format_args!($($arg)*));
            let formatted_message = format_log_message(&new_time_string(), "Init", text, false);
            eprintln!("{formatted_message}");
        }
    }};
}

#[macro_export]
macro_rules! error {
    ($config:expr, $($arg:tt)*) => {{
        #[cfg(feature = "log-err")]
        {
            if $config.log_level >= Level::Error {
                error($config, &std::fmt::format(format_args!($($arg)*)));
            }
        }
    }};
}

#[macro_export]
macro_rules! warn {
    ($config:expr, $($arg:tt)*) => {{
        #[cfg(feature = "log-warn")]
        {
            if $config.log_level >= Level::Warn {
                warn($config, &std::fmt::format(format_args!($($arg)*)));
            }
        }
    }};
}

#[macro_export]
macro_rules! info {
    ($config:expr, $($arg:tt)*) => {{
        #[cfg(feature = "log-info")]
        {
            if $config.log_level >= Level::Info {
                info($config, &std::fmt::format(format_args!($($arg)*)));
            }
        }
    }};
}

#[macro_export]
macro_rules! verb {
    ($config:expr, $($arg:tt)*) => {{
        #[cfg(feature = "log-verb")]
        {
            if $config.log_level >= Level::Verb {
                verb($config, &std::fmt::format(format_args!($($arg)*)));
            }
        }
    }};
}

#[macro_export]
macro_rules! debug {
    ($config:expr, $($arg:tt)*) => {{
        #[cfg(feature = "log-debug")]
        {
            if $config.log_level >= Level::Debug {
                debug($config, &std::fmt::format(format_args!($($arg)*)));
            }
        }
    }};
}

#[macro_export]
macro_rules! trace {
    ($config:expr, $($arg:tt)*) => {{
        #[cfg(feature = "log-trace")]
        {
            if $config.log_level >= Level::Trace {
                trace($config, &std::fmt::format(format_args!($($arg)*)));
            }
        }
    }};
}
