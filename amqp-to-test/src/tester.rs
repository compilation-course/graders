use failure::{bail, Error, ResultExt};
use futures::channel::mpsc::{Receiver, Sender};
use futures::{SinkExt, StreamExt, TryFutureExt};
use graders_utils::amqputils::{AMQPRequest, AMQPResponse};
use std::collections::btree_map::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::config;

#[derive(Fail, Debug)]
#[fail(display = "Execution error: {}", _0)]
pub struct ExecutionError(String);

#[derive(Deserialize)]
pub struct TesterConfiguration {
    pub docker_image: String,
    pub dir_on_host: PathBuf,
    pub dir_in_docker: PathBuf,
    pub env: Option<BTreeMap<String, BTreeMap<String, String>>>,
    pub extra_args: Option<Vec<String>>,
    pub parallelism: usize,
    pub program: PathBuf,
    pub test_files: BTreeMap<String, PathBuf>,
}

/// Execute a request using docker on a blocking CPU pool. Return the
/// YAML output or a descriptive error.
async fn execute(
    config: &TesterConfiguration,
    request: &AMQPRequest,
    cpu_access: Arc<Semaphore>,
) -> Result<String, Error> {
    let test_file = match config.test_files.get(&request.lab) {
        Some(file) => config.dir_in_docker.join(&file),
        None => {
            bail!(
                "unable to find configuration for lab {} for {}",
                request.lab,
                request.job_name
            );
        }
    };
    let request = request.clone();
    let program = config.dir_in_docker.join(&config.program);
    let dir_on_host = config.dir_on_host.clone();
    let dir_in_docker = config.dir_in_docker.clone();
    let env = config
        .env
        .clone()
        .unwrap_or_else(BTreeMap::new)
        .get(&request.lab)
        .cloned()
        .unwrap_or_else(BTreeMap::new)
        .iter()
        .flat_map(|(k, v)| vec!["-e".to_owned(), format!("{}={}", k, v)])
        .collect::<Vec<_>>();
    let docker_image = config.docker_image.clone();
    let extra_args = config.extra_args.clone().unwrap_or_else(Vec::new);
    let _permit = cpu_access.acquire().await;
    tokio::task::spawn_blocking(move || {
        info!("starting docker command for {}", request.job_name);
        let mut command = Command::new("docker");
        let command = command
            .arg("run")
            .arg("--rm")
            .arg("-v")
            .arg(&format!(
                "{}:{}",
                dir_on_host.to_str().unwrap(),
                dir_in_docker.to_str().unwrap()
            ))
            .args(env)
            .arg(&docker_image)
            .args(extra_args)
            .arg(&request.zip_url)
            .arg(&request.dir)
            .arg(&program)
            .arg(&test_file);
        trace!("docker command for {}: {:?}", request.job_name, command);
        let output = command
            .stdin(Stdio::null())
            .output()
            .context("cannot run command")?;
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
    })
    .await?
}

/// Execute a request using docker and build a response containing the
/// YAML output or response.
async fn execute_request(
    config: &TesterConfiguration,
    request: AMQPRequest,
    cpu_access: Arc<Semaphore>,
) -> AMQPResponse {
    let yaml = match execute(config, &request, cpu_access).await {
        Ok(y) => y,
        Err(e) => yaml_error(&e),
    };
    AMQPResponse {
        job_name: request.job_name,
        lab: request.lab,
        opaque: request.opaque,
        yaml_result: yaml,
        result_queue: request.result_queue,
        delivery_tag: request.delivery_tag.unwrap(),
    }
}

#[derive(Serialize)]
struct ExecutionErrorReport {
    grade: usize,
    #[serde(rename = "max-grade")]
    max_grade: usize,
    explanation: String,
}

fn yaml_error(error: &Error) -> String {
    serde_yaml::to_string(&ExecutionErrorReport {
        grade: 0,
        max_grade: 1,
        explanation: error.to_string(),
    })
    .unwrap()
}

/// Start the executors on the current thread
pub async fn start_executor(
    config: &Arc<config::Configuration>,
    receive_request: Receiver<AMQPRequest>,
    send_response: Sender<AMQPResponse>,
) {
    let cpu_access = Arc::new(Semaphore::new(config.tester.parallelism));
    receive_request
        .for_each(move |request| {
            let cpu_access = cpu_access.clone();
            let send_response = send_response.clone();
            async move {
                debug!("received request {:?}", request);
                let mut send_response = send_response.clone();
                let config = config.clone();
                tokio::spawn(async move {
                    let response = execute_request(&config.tester, request, cpu_access).await;
                    send_response
                        .send(response)
                        .inspect_err(|e| {
                            error!("unable to send AMQPResponse to queue: {}", e);
                        })
                        .await
                });
            }
        })
        .await;
}
