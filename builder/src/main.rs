extern crate env_logger;
extern crate graders_utils;
#[macro_use]
extern crate log;
extern crate mktemp;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
#[macro_use]
extern crate structopt;

mod commands;
mod outputs;

use graders_utils::ziputils::unzip;
use mktemp::Temp;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

/// Compile the compiler from the given directory, set env
/// DTIGER environment variable and run the tests using the
/// given tester as well as the given configuration file.
///
/// It outputs a YAML file with the result.
///
/// Logging is enabled by setting `RUST_LOG` to the desired
/// level, possibly restricted to this program:
/// `RUST_LOG=builder=trace ./builder …` will generate many traces.
#[derive(StructOpt)]
#[structopt(name = "builder")]
pub struct Opt {
    /// Use a non-standard LLVM
    #[structopt(name = "llvm lib directory", long = "with-llvm", parse(from_os_str))]
    with_llvm: Option<PathBuf>,

    /// Run tests in verbose mode
    #[structopt(short = "v", long = "verbose")]
    verbose: bool,

    /// Output file to use instead of standard out
    #[structopt(short = "o", long = "output", parse(from_os_str))]
    output_file: Option<PathBuf>,

    /// Compiler source (directory, zip file, or URL of zip file)
    #[structopt(name = "compiler location")]
    src: String,

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
    let tmp = Temp::new_dir().unwrap();
    if !Path::new(&opt.src).is_dir() {
        if opt.src.ends_with(".zip") {
            info!("Unzipping {:?}", opt.src);
            match unzip(&tmp.to_path_buf(), &opt.src) {
                Ok(d) => opt.src = d.to_str().unwrap().to_owned(), // Replace src by directory
                Err(e) => {
                    outputs::write_error(&opt, format!("cannot extract zip file: {}", e));
                    return;
                }
            }
        } else {
            error!("unknown repository source {}", opt.src);
            outputs::write_error(&opt, format!("unknown repository source {}", opt.src));
            return;
        }
    }
    let dtiger = Path::new(&opt.src).join("src/driver/dtiger");
    match commands::build(&opt).and_then(|_| commands::run_test(&opt, &dtiger)) {
        Ok(output) => outputs::write_output(&opt, &output),
        Err(s) => outputs::write_error(&opt, s),
    }
}
