use serde_yaml;
use std;

error_chain! {
    errors {
        ExecutionError(t: String) {
            description("execution error")
            display("execution error: {}", t)
        }
    }

    foreign_links {
        Io(std::io::Error);
        Yaml(serde_yaml::Error);
    }
}
