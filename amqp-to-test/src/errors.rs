use std;
use toml;

error_chain! {
    errors {
        ExecutionError(t: String) {
            description("execution error")
            display("execution error: {}", t)
        }
    }

    foreign_links {
        Io(std::io::Error);
        Toml(toml::de::Error);
    }
}
