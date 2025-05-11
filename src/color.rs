use crate::util;
use std::fmt;

pub struct ColorizedText {
    pub text: String,
    #[cfg(feature = "log-trace")] pub color_code_length: usize,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Color {
    Reset = 1,
    Red,
    Yellow,
    Green,
    Magenta,
    Blue,
    Cyan,
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.write_foreground_code();
        util::format_with_options(&value, f)
    }
}

impl Color {
    fn write_foreground_code(&self) -> &str {
        match self {
            Color::Reset => "\x1B[0m",
            Color::Red => "\x1B[31m",
            Color::Yellow => "\x1B[33m",
            Color::Green => "\x1B[32m",
            Color::Magenta => "\x1B[35m",
            Color::Blue => "\x1B[34m",
            Color::Cyan => "\x1B[36m",
        }
    }

    fn colorize(self, text: &str) -> ColorizedText {
        let color = self.write_foreground_code();
        let reset_color = Color::Reset.write_foreground_code();
        let text = format!("{color}{text}{reset_color}");
        #[cfg(feature = "log-trace")] let color_code_length = color.len() + reset_color.len();

        ColorizedText {
            text,
            #[cfg(feature = "log-trace")] color_code_length,
        }
    }
}

pub trait Colorize {
    #[allow(dead_code)]
    fn colorize(&self, color: Color) -> ColorizedText;
    fn red(&self) -> ColorizedText;
    fn yellow(&self) -> ColorizedText;
    fn green(&self) -> ColorizedText;
    fn magenta(&self) -> ColorizedText;
    fn blue(&self) -> ColorizedText;
    fn cyan(&self) -> ColorizedText;
}

impl Colorize for String {
    fn colorize(&self, color: Color) -> ColorizedText {
        color.colorize(self)
    }

    fn red(&self) -> ColorizedText {
        Color::Red.colorize(self)
    }

    fn yellow(&self) -> ColorizedText {
        Color::Yellow.colorize(self)
    }

    fn green(&self) -> ColorizedText {
        Color::Green.colorize(self)
    }

    fn magenta(&self) -> ColorizedText {
        Color::Magenta.colorize(self)
    }

    fn blue(&self) -> ColorizedText {
        Color::Blue.colorize(self)
    }

    fn cyan(&self) -> ColorizedText {
        Color::Cyan.colorize(self)
    }
}
