use super::Opt;
use is_executable::IsExecutable;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(thiserror::Error, Debug)]
pub enum RunError {
    #[error("cannot build program in `{1}`")]
    CannotBuildProgram(#[source] std::io::Error, String),
    #[error("cannot run tests {0:?} -y {1:?}")]
    CannotRunTests(#[source] std::io::Error, PathBuf, PathBuf),
    #[error("some files (e.g, `{0}`) should be executable, but the executable bit is not set")]
    NotExecutable(PathBuf),
    #[error("{0}")]
    Other(String),
}

pub fn run_test(opt: &Opt, dtiger: &Path) -> Result<String, RunError> {
    log::info!(
        "executing {:?} with test source {:?} on executable {:?}",
        opt.test_command,
        opt.test_file,
        dtiger
    );
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
            RunError::CannotRunTests(e, opt.test_command.clone(), opt.test_file.clone())
        })?;
    if output.status.code() == Some(0) {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        log::warn!(
            "received status code {:?} when running tests",
            output.status.code()
        );
        Err(RunError::Other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}

fn exec(opt: &Opt, command: &str) -> Result<(), RunError> {
    exec_args(opt, command, &[])
}

fn exec_args(opt: &Opt, command: &str, args: &[&str]) -> Result<(), RunError> {
    log::info!("executing {} with args {:?}", command, args);
    let output = Command::new(command)
        .args(args)
        .current_dir(&opt.src)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output()
        .map_err(|e| RunError::CannotBuildProgram(e, opt.src.clone()))?;
    log::trace!(
        "command {} with args {:?} terminated with status {:?}",
        command,
        args,
        output
    );
    if output.status.code() == Some(0) {
        Ok(())
    } else {
        Err(RunError::Other(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
    }
}

fn make(opt: &Opt) -> Result<(), RunError> {
    exec(opt, "make")
}

fn configure(opt: &Opt) -> Result<(), RunError> {
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

fn check_executable_bits(opt: &Opt) -> Result<(), RunError> {
    for &p in &["configure", "autogen.sh"] {
        let path = [&opt.src, p].iter().copied().collect::<PathBuf>();
        if path.exists() && !path.is_executable() {
            return Err(RunError::NotExecutable(path));
        }
    }
    Ok(())
}

fn autogen(opt: &Opt) -> Result<(), RunError> {
    exec(opt, "./autogen.sh").and_then(|_| configure(opt))
}

pub fn build(opt: &Opt) -> Result<(), RunError> {
    check_executable_bits(opt)?;
    make(opt)
        .or_else(|_| configure(opt))
        .or_else(|_| autogen(opt))
}
