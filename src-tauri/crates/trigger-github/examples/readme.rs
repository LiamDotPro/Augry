//! Prints the Kinds table for README.md.

fn main() {
    print!("{}", trigger_core::kinds_markdown(trigger_github::KINDS));
}
