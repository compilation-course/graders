mod commands;
mod outputs;

use clap::Parser;
use graders_utils::ziputils::unzip;
use std::path::{Path, PathBuf};

/// Compile the compiler from the given directory, set env
/// DTIGER environment variable and run the tests using the
/// given tester as well as the given configuration file.
///
/// It outputs a YAML file with the result.
///
/// Logging is enabled by setting `RUST_LOG` to the desired
/// level, possibly restricted to this program:
/// `RUST_LOG=builder=trace ./builder …` will generate many traces.
#[derive(Parser)]
pub struct Opt {
    /// Use a non-standard LLVM
    #[clap(long)]
    with_llvm: Option<PathBuf>,

    /// Run tests in verbose mode
    #[clap(short, long)]
    verbose: bool,

    /// Output file to use instead of standard out
    #[clap(short, long)]
    output_file: Option<PathBuf>,

    /// Compiler source (directory, zip file, or URL of zip file)
    src: String,

    /// Name of the mandatory top-level directory in the zip file
    top_level_dir: PathBuf,

    /// Test driver command
    test_command: PathBuf,

    /// Test YAML configuration file
    test_file: PathBuf,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let mut opt = Opt::parse();
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
