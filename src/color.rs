// vim: sw=4 et filetype=rust

#![cfg(feature = "color")]

use crate::util;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
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

    fn colorize(self, text: &str) -> String {
        format!(
            "{}{}{}",
            self.write_foreground_code(),
            text,
            Color::Reset.write_foreground_code()
        )
    }
}

pub trait Colorize {
    #[allow(dead_code)]
    fn colorize(&self, color: Color) -> String;
    fn red(&self) -> String;
    fn yellow(&self) -> String;
    fn green(&self) -> String;
    fn magenta(&self) -> String;
    fn blue(&self) -> String;
    fn cyan(&self) -> String;
}

impl Colorize for &str {
    fn colorize(&self, color: Color) -> String {
        color.colorize(self)
    }

    fn red(&self) -> String {
        Color::Red.colorize(self)
    }

    fn yellow(&self) -> String {
        Color::Yellow.colorize(self)
    }

    fn green(&self) -> String {
        Color::Green.colorize(self)
    }

    fn magenta(&self) -> String {
        Color::Magenta.colorize(self)
    }

    fn blue(&self) -> String {
        Color::Blue.colorize(self)
    }

    fn cyan(&self) -> String {
        Color::Cyan.colorize(self)
    }
}
