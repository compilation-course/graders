use config;
use errors::{self, ResultExt};
use errors::ErrorKind::ExecutionError;
use futures::{future, Future, Sink, Stream};
use futures::sync::mpsc::{Receiver, Sender};
use futures_cpupool::CpuPool;
use graders_utils::amqputils::{AMQPRequest, AMQPResponse};
use serde_yaml;
use std::collections::btree_map::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::executor::current_thread;

#[derive(Deserialize)]
pub struct TesterConfiguration {
    pub docker_image: String,
    pub dir_on_host: PathBuf,
    pub dir_in_docker: PathBuf,
    pub extra_args: Option<Vec<String>>,
    pub parallelism: usize,
    pub program: PathBuf,
    pub test_files: BTreeMap<String, PathBuf>,
}

/// Execute a request using docker in the given CPU pool. Return the
/// YAML output or a descriptive error.
fn execute(
    config: &TesterConfiguration,
    request: &AMQPRequest,
    cpu_pool: &CpuPool,
) -> Box<Future<Item = String, Error = errors::Error>> {
    let test_file = match config.test_files.get(&request.step) {
        Some(file) => config.dir_in_docker.join(&file),
        None => {
            return Box::new(future::err(
                format!(
                    "unable to find configuration for step {} for {}",
                    request.step, request.job_name
                ).into(),
            ))
        }
    };
    let request = request.clone();
    let program = config.dir_in_docker.join(&config.program);
    let dir_on_host = config.dir_on_host.clone();
    let dir_in_docker = config.dir_in_docker.clone();
    let docker_image = config.docker_image.clone();
    let extra_args = config.extra_args.clone().unwrap_or_else(|| vec![]);
    Box::new(cpu_pool.spawn_fn(move || {
        info!("starting docker command for {}", request.job_name);
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
            .args(extra_args)
            .arg(&request.zip_url)
            .arg(&program)
            .arg(&test_file)
            .stdin(Stdio::null())
            .output()
            .chain_err(|| ExecutionError("cannot run command".to_owned()))?;
        if output.status.code() == Some(0) {
            info!(
                "docker command for {} finished succesfully",
                request.job_name
            );
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            warn!(
                "docker command for {} finished with an error",
                request.job_name
            );
            Err(ExecutionError(String::from_utf8_lossy(&output.stderr).to_string()).into())
        }
    }))
}

/// Execute a request using docker and build a response containing the
/// YAML output or response.
fn execute_request(
    config: &TesterConfiguration,
    request: AMQPRequest,
    cpu_pool: &CpuPool,
) -> Box<Future<Item = AMQPResponse, Error = ()>> {
    Box::new(
        execute(config, &request, cpu_pool)
            .then(|result| match result {
                Ok(y) => future::ok(y),
                Err(e) => future::ok(yaml_error(&e)),
            })
            .map(move |yaml| AMQPResponse {
                job_name: request.job_name,
                step: request.step,
                opaque: request.opaque,
                yaml_result: yaml,
                result_queue: request.result_queue,
                delivery_tag: request.delivery_tag.unwrap(),
            }),
    )
}

#[derive(Serialize)]
struct ExecutionErrorReport {
    grade: usize,
    #[serde(rename = "max-grade")]
    max_grade: usize,
    explanation: String,
}

fn yaml_error(error: &errors::Error) -> String {
    serde_yaml::to_string(&ExecutionErrorReport {
        grade: 0,
        max_grade: 1,
        explanation: error.to_string(),
    }).unwrap()
}

/// Start the executors on the current thread
pub fn start_executor(
    config: &Arc<config::Configuration>,
    receive_request: Receiver<AMQPRequest>,
    send_response: Sender<AMQPResponse>,
) -> Box<Future<Item = (), Error = ()>> {
    let cpu_pool = CpuPool::new(config.tester.parallelism);
    let config = config.clone();
    Box::new(receive_request.for_each(move |request| {
        debug!("received request {:?}", request);
        let send_response = send_response.clone();
        current_thread::spawn(
            execute_request(&config.tester, request, &cpu_pool)
                .and_then(move |response| {
                    send_response.send(response).map_err(|e| {
                        error!("unable to send AMQPResponse to queue: {}", e);
                        ()
                    })
                })
                .map(|_| ()),
        );
        future::ok(())
    }))
}
