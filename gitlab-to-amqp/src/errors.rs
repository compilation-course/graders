use git2;
use hyper;
use std;
use serde_json;
use serde_yaml;

error_chain! {
    errors {
        WebServerCrash {
            description("web server crash")
        }
    }

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
