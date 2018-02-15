//! Try the following in order, and stop as soon as it works:
//!
//!   - `make`
//!   - `./configure && make`
//!   - `./autogen.sh && ./configure && make`
//!
//! Exit with success if one command does, mirror the stderr
//! (or give a meaningful error message) of the latest command
//! tried otherwise.

extern crate env_logger;
#[macro_use]
extern crate log;
extern crate mktemp;
extern crate reqwest;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
#[macro_use]
extern crate structopt;
extern crate zip;

mod commands;
mod outputs;
mod ziputils;

use mktemp::Temp;
use std::path::PathBuf;
use structopt::StructOpt;
use ziputils::unzip;

/// Compile the compiler from the given directory, set env
/// DTIGER environment variable and run the tests using the
/// given tester as well as the given configuration file.
///
/// It outputs a YAML file with the result.
#[derive(StructOpt)]
#[structopt(name = "builder")]
pub struct Opt {
    /// Use a non-standard LLVM
    #[structopt(name = "llvm lib directory", long = "with-llvm", parse(from_os_str))]
    with_llvm: Option<PathBuf>,

    /// Get source from a zip file
    #[structopt(name = "zip file", long = "--zip", short = "-z")]
    zip_file: Option<String>,

    /// Run tests in verbose mode
    #[structopt(short = "v", long = "verbose")]
    verbose: bool,

    /// Output file to use instead of standard out
    #[structopt(short = "o", long = "output", parse(from_os_str))]
    output_file: Option<PathBuf>,

    /// Top-level dragon-tiger directory. Zip file will be unzipped in a subdirectory if specified
    #[structopt(name = "compiler directory", parse(from_os_str))]
    src_dir: PathBuf,

    /// Test driver command
    #[structopt(name = "test command", parse(from_os_str))]
    test_command: PathBuf,

    /// Test YAML configuration file
    #[structopt(name = "test configuration file", parse(from_os_str))]
    test_file: PathBuf,
}

fn main() {
    env_logger::init();
    let mut opt = Opt::from_args();
    let tmp = Temp::new_dir_in(&opt.src_dir).unwrap();
    if let Some(ref zip_file) = opt.zip_file {
        info!("Unzipping {}", zip_file);
        match unzip(&tmp.to_path_buf(), zip_file) {
            Ok(d) => opt.src_dir = d,
            Err(e) => {
                outputs::write_error(&opt, format!("cannot extract zip file: {}", e));
                return;
            }
        }
    }
    let dtiger = opt.src_dir.join("src/driver/dtiger");
    match commands::build(&opt).and_then(|_| commands::run_test(&opt, &dtiger)) {
        Ok(output) => outputs::write_output(&opt, &output),
        Err(s) => outputs::write_error(&opt, s),
    }
}
