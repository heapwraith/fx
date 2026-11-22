//! Interface language (task T3.7): Portuguese (default) or English, one
//! process-wide switch read by every text and number format.
//!
//! Texts are written in pairs at the place they are used, so the compiler
//! guarantees both versions exist:
//! - `tr!("Copiar", "Copy")`: a `&'static str` (no `{}` inside);
//! - `trf!("Moveu {n} itens", "Moved {n} items")`: a `String`, with the
//!   same arguments and captures as `format!`.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Whether the interface is in English.
pub fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn set_english(on: bool) {
    ENGLISH.store(on, Ordering::Relaxed);
}

/// A fixed text in Portuguese and English.
#[macro_export]
macro_rules! tr {
    ($pt:literal, $en:literal $(,)?) => {
        if $crate::locale::english() { $en } else { $pt }
    };
}

/// A formatted text in Portuguese and English (`format!` rules).
#[macro_export]
macro_rules! trf {
    ($pt:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::locale::english() {
            format!($en $(, $arg)*)
        } else {
            format!($pt $(, $arg)*)
        }
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn pairs_follow_the_switch() {
        // Only the Portuguese side here: the switch is process-wide and other
        // tests run in parallel expecting Portuguese.
        let n = 3;
        assert_eq!(tr!("Copiar", "Copy"), "Copiar");
        assert_eq!(trf!("{n} itens", "{n} items"), "3 itens");
        assert_eq!(trf!("{} de {}", "{} of {}", 1, 2), "1 de 2");
    }
}
