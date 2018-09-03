use git2;
use hyper;
use serde_json;
use serde_yaml;
use std;

error_chain! {
    foreign_links {
        AddrParse(std::net::AddrParseError);
        Git(git2::Error);
        Hyper(hyper::Error);
        Io(std::io::Error);
        Json(serde_json::Error);
        ParseInt(std::num::ParseIntError);
        Yaml(serde_yaml::Error);
    }
}
