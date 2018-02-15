//! Try the following in order, and stop as soon as it works:
//!
//!   - `make`
//!   - `./configure && make`
//!   - `./autogen.sh && ./configure && make`
//!
//! Exit with success if one command does, mirror the stderr
//! (or give a meaningful error message) of the latest command
//! tried otherwise.

#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
#[macro_use]
extern crate structopt;

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::{exit, Command, Stdio};
use structopt::StructOpt;

/// Compile the compiler from the given directory, set env
/// DTIGER environment variable and run the tests using the
/// given tester as well as the given configuration file.
///
/// It outputs a YAML file with the result.
#[derive(StructOpt)]
#[structopt(name = "builder")]
struct Opt {
    /// Use a non-standard LLVM
    #[structopt(name = "llvm lib directory", long = "with-llvm", parse(from_os_str))]
    with_llvm: Option<PathBuf>,

    /// Run tests in verbose mode
    #[structopt(short = "v", long = "verbose")]
    verbose: bool,

    /// Output file to use instead of standard out
    #[structopt(short = "o", long = "output", parse(from_os_str))]
    output_file: Option<PathBuf>,

    /// Top-level dragon-tiger directory
    #[structopt(name = "compiler directory", parse(from_os_str))]
    src_dir: PathBuf,

    /// Test driver command
    #[structopt(name = "test command", parse(from_os_str))]
    test_command: PathBuf,

    /// Test YAML configuration file
    #[structopt(name = "test configuration file", parse(from_os_str))]
    test_file: PathBuf,
}

#[derive(Serialize)]
struct Output {
    grade: u32,
    #[serde(rename = "max-grade")]
    max_grade: u32,
    explanation: String,
}

fn write_file(file: &PathBuf, output: &str) -> std::io::Result<()> {
    let mut file = File::create(file)?;
    file.write_all(output.as_bytes())
}

fn write_output(opt: &Opt, output: &str) {
    if let Some(ref output_file) = opt.output_file {
        if let Err(e) = write_file(output_file, output) {
            eprintln!("cannot write output file {:?}: {}", output_file, e);
            exit(1);
        }
    } else {
        println!("{}", output);
    }
}

fn write_error(opt: &Opt, message: String) {
    write_output(
        opt,
        &serde_yaml::to_string(&Output {
            grade: 0,
            max_grade: 1,
            explanation: message,
        }).unwrap(),
    );
}

fn main() {
    let opt = Opt::from_args();
    let mut dtiger = opt.src_dir.clone();
    dtiger.push("src");
    dtiger.push("driver");
    dtiger.push("dtiger");
    match build(&opt).and_then(|_| run_test(&opt, &dtiger)) {
        Ok(output) => write_output(&opt, &output),
        Err(s) => write_error(&opt, s),
    }
}

fn run_test(opt: &Opt, dtiger: &PathBuf) -> Result<String, String> {
    let mut output = Command::new(&opt.test_command);
    if opt.verbose {
        output.arg("-v");
    }
    let output = output
        .arg("-y")
        .arg(&opt.test_file)
        .current_dir(opt.test_file.parent().unwrap())
        .env("DTIGER", dtiger)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            format!(
                "cannot run tests {:?} -y {:?}: {}",
                opt.test_command, opt.test_file, e
            )
        })?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn exec(opt: &Opt, command: &str) -> Result<(), String> {
    exec_args(opt, command, &[])
}

fn exec_args(opt: &Opt, command: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(command)
        .args(args)
        .current_dir(&opt.src_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output()
        .map_err(|e| format!("cannot build program in {:?}: {}", opt.src_dir, e))?;
    if output.status.code() == Some(0) {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

fn make(opt: &Opt) -> Result<(), String> {
    exec(opt, "make")
}

fn configure(opt: &Opt) -> Result<(), String> {
    let configure = if let Some(ref d) = opt.with_llvm {
        exec_args(
            opt,
            "./configure",
            &[&format!("--with-llvm={}", d.to_str().unwrap())],
        )
    } else {
        exec(opt, "./configure")
    };
    configure.and_then(|_| make(opt))
}

fn autogen(opt: &Opt) -> Result<(), String> {
    exec(opt, "./autogen.sh").and_then(|_| configure(opt))
}

fn build(opt: &Opt) -> Result<(), String> {
    make(opt)
        .or_else(|_| configure(opt))
        .or_else(|_| autogen(opt))
}
