use super::Opt;
use failure::{Error, ResultExt};
use is_executable::IsExecutable;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Fail, Debug)]
#[fail(display = "fatal error when running test: {}", _0)]
pub struct RunError(String);

pub fn run_test(opt: &Opt, dtiger: &Path) -> Result<String, Error> {
    info!(
        "executing {:?} with test source {:?} on executable {:?}",
        opt.test_command, opt.test_file, dtiger
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
        .context(format!(
            "cannot run tests {:?} -y {:?}",
            opt.test_command, opt.test_file
        ))?;
    if output.status.code() == Some(0) {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        warn!(
            "received status code {:?} when running tests",
            output.status.code()
        );
        Err(RunError(String::from_utf8_lossy(&output.stderr).to_string()).into())
    }
}

fn exec(opt: &Opt, command: &str) -> Result<(), Error> {
    exec_args(opt, command, &[])
}

fn exec_args(opt: &Opt, command: &str, args: &[&str]) -> Result<(), Error> {
    info!("executing {} with args {:?}", command, args);
    let output = Command::new(command)
        .args(args)
        .current_dir(&opt.src)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output()
        .context(format!("cannot build program in {:?}", opt.src))?;
    trace!(
        "command {} with args {:?} terminated with status {:?}",
        command,
        args,
        output
    );
    if output.status.code() == Some(0) {
        Ok(())
    } else {
        Err(format_err!("{}", String::from_utf8_lossy(&output.stderr)))
    }
}

fn make(opt: &Opt) -> Result<(), Error> {
    exec(opt, "make")
}

fn configure(opt: &Opt) -> Result<(), Error> {
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

fn check_executable_bits(opt: &Opt) -> Result<(), Error> {
    for &p in &["configure", "autogen.sh"] {
        let path = [&opt.src, p].iter().cloned().collect::<PathBuf>();
        if path.exists() && !path.is_executable() {
            bail!(
                "Some files (e.g, {}) should be executable, but the executable bit is not set",
                p
            );
        }
    }
    Ok(())
}

fn autogen(opt: &Opt) -> Result<(), Error> {
    exec(opt, "./autogen.sh").and_then(|_| configure(opt))
}

pub fn build(opt: &Opt) -> Result<(), Error> {
    check_executable_bits(opt)?;
    make(opt)
        .or_else(|_| configure(opt))
        .or_else(|_| autogen(opt))
}
