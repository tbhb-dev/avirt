//! Writes to the process's standard output and standard error.
//!
//! The functions ignore a failed write, such as one to a closed pipe. The programs print progress,
//! and no other stream exists for a report of that failure.

use core::fmt::Arguments;
use std::io::{Write as _, stderr, stdout};

/// Writes `args` and a newline to standard error.
pub fn err_line(args: Arguments<'_>) {
    drop(writeln!(stderr(), "{args}"));
}

/// Writes `args` to standard output.
pub fn out(args: Arguments<'_>) {
    drop(stdout().write_fmt(args));
}

/// Writes `args` and a newline to standard output.
pub fn out_line(args: Arguments<'_>) {
    drop(writeln!(stdout(), "{args}"));
}
