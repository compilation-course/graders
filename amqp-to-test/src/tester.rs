use errors::{self, ResultExt};
use errors::ErrorKind::ExecutionError;
use futures::{future, Future};
use futures_cpupool::CpuPool;
use graders_utils::amqputils::AMQPRequest;
use std::collections::btree_map::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Deserialize)]
pub struct TesterConfiguration {
    pub docker_image: String,
    pub dir_on_host: PathBuf,
    pub dir_in_docker: PathBuf,
    pub program: PathBuf,
    pub test_files: BTreeMap<String, TestConfiguration>,
    pub parallelism: usize,
}

#[derive(Deserialize)]
pub struct TestConfiguration {
    pub file: PathBuf,
}

fn execute(
    config: &TesterConfiguration,
    request: AMQPRequest,
    cpu_pool: &CpuPool,
) -> Box<Future<Item = String, Error = errors::Error>> {
    let test_file = match config.test_files.get(&request.step) {
        Some(step) => config.dir_in_docker.join(&step.file),
        None => {
            return Box::new(future::err(
                format!("unable to find configuration for step {}", request.step).into(),
            ))
        }
    };
    let program = config.dir_in_docker.join(&config.program);
    let dir_on_host = config.dir_on_host.clone();
    let dir_in_docker = config.dir_in_docker.clone();
    let docker_image = config.docker_image.clone();
    Box::new(cpu_pool.spawn_fn(move || {
        let output = Command::new("docker")
            .arg("run")
            .arg("--rm")
            .arg("-v")
            .arg(&format!(
                "{}:{}",
                dir_on_host.to_str().unwrap(),
                dir_in_docker.to_str().unwrap()
            ))
            .arg(&docker_image)
            .arg(&request.zip_url)
            .arg(&program)
            .arg(&test_file)
            .stdin(Stdio::null())
            .output()
            .chain_err(|| ExecutionError("cannot run command".to_owned()))?;
        if output.status.code() == Some(0) {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(ExecutionError(String::from_utf8_lossy(&output.stderr).to_string()).into())
        }
    }))
}
