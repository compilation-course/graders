mod commands;
mod outputs;

use graders_utils::ziputils::unzip;
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

    /// Name of the mandatory top-level directory in the zip file
    #[structopt(name = "top-level directory", parse(from_os_str))]
    top_level_dir: PathBuf,

    /// Test driver command
    #[structopt(name = "test command", parse(from_os_str))]
    test_command: PathBuf,

    /// Test YAML configuration file
    #[structopt(name = "test configuration file", parse(from_os_str))]
    test_file: PathBuf,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let mut opt = Opt::from_args();
    let tmp = tempfile::TempDir::new().unwrap();
    let top_level_dir = opt
        .top_level_dir
        .to_str()
        .expect("non-utf8 character in top-level directory");
    if !Path::new(&opt.src).is_dir() {
        log::info!("Unzipping {:?}", opt.src);
        match unzip(&tmp.into_path(), &opt.src, top_level_dir).await {
            Ok(d) => opt.src = d.to_str().unwrap().to_owned(), // Replace src by directory
            Err(e) => {
                outputs::write_error(&opt, e.into());
                return;
            }
        }
    }
    let dtiger = Path::new(&opt.src).join("src/driver/dtiger");
    match commands::build(&opt).and_then(|_| commands::run_test(&opt, &dtiger)) {
        Ok(output) => outputs::write_output(&opt, &output),
        Err(e) => outputs::write_error(&opt, e.into()),
    }
}
