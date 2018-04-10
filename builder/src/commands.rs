use super::Opt;
use errors::ErrorKind::RunError;
use errors::Result;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn run_test(opt: &Opt, dtiger: &Path) -> Result<String> {
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
        .map_err(|e| {
            format!(
                "cannot run tests {:?} -y {:?}: {}",
                opt.test_command, opt.test_file, e
            )
        })?;
    if output.status.code() == Some(0) {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        warn!(
            "received status code {:?} when running tests",
            output.status.code()
        );
        bail!(RunError(
            String::from_utf8_lossy(&output.stderr).to_string()
        ))
    }
}

fn exec(opt: &Opt, command: &str) -> Result<()> {
    exec_args(opt, command, &[])
}

fn exec_args(opt: &Opt, command: &str, args: &[&str]) -> Result<()> {
    info!("executing {} with args {:?}", command, args);
    let output = Command::new(command)
        .args(args)
        .current_dir(&opt.src)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .output()
        .map_err(|e| format!("cannot build program in {:?}: {}", opt.src, e))?;
    trace!(
        "command {} with args {:?} terminated with status {:?}",
        command,
        args,
        output
    );
    if output.status.code() == Some(0) {
        Ok(())
    } else {
        bail!(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn make(opt: &Opt) -> Result<()> {
    exec(opt, "make")
}

fn configure(opt: &Opt) -> Result<()> {
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

fn autogen(opt: &Opt) -> Result<()> {
    exec(opt, "./autogen.sh").and_then(|_| configure(opt))
}

pub fn build(opt: &Opt) -> Result<()> {
    make(opt)
        .or_else(|_| configure(opt))
        .or_else(|_| autogen(opt))
}
