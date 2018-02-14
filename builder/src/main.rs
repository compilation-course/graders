//! Try the following in order, and stop as soon as it works:
//!
//!   - `make`
//!   - `./configure && make`
//!   - `./autogen.sh && ./configure && make`
//!
//! Exit with success if one command does, mirror the stderr
//! (or give a meaningful error message) of the latest command
//! tried otherwise.

use std::process::{exit, Command, Stdio};

fn main() {
    match build() {
        Ok(_) => (),
        Err(s) => {
            eprintln!("{}", s);
            exit(1);
        }
    }
}

fn exec(command: &str) -> Result<(), String> {
    let output = Command::new(command)
        .stdout(Stdio::null())
        .output()
        .map_err(|e| format!("cannot build program: {}", e))?;
    if output.status.code() == Some(0) {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

fn make() -> Result<(), String> {
    exec("make")
}

fn configure() -> Result<(), String> {
    exec("./configure").and_then(|_| make())
}

fn autogen() -> Result<(), String> {
    exec("./autogen.sh").and_then(|_| configure())
}

fn build() -> Result<(), String> {
    make().or_else(|_| configure()).or_else(|_| autogen())
}
