use std::collections::btree_map::BTreeMap;
use std::path::PathBuf;

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
