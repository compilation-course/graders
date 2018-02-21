use std;
use toml;

error_chain! {
    foreign_links {
        Io(std::io::Error);
        Toml(toml::de::Error);
    }
}
