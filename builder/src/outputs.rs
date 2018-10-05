use failure::Error;
use serde_yaml;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::process::exit;

use super::Opt;

#[derive(Serialize)]
struct Output {
    grade: u32,
    #[serde(rename = "max-grade")]
    max_grade: u32,
    explanation: String,
}

fn write_file<P: AsRef<Path>>(file: P, output: &str) -> io::Result<()> {
    let mut file = File::create(file)?;
    file.write_all(output.as_bytes())
}

pub fn write_output(opt: &Opt, output: &str) {
    if let Some(ref output_file) = opt.output_file {
        if let Err(e) = write_file(output_file, output) {
            error!("cannot write output file {:?}: {}", output_file, e);
            exit(1);
        }
    } else {
        println!("{}", output);
    }
}

pub fn write_error(opt: &Opt, error: &Error) {
    write_output(
        opt,
        &serde_yaml::to_string(&Output {
            grade: 0,
            max_grade: 1,
            explanation: error.to_string(),
        })
        .unwrap(),
    );
}
