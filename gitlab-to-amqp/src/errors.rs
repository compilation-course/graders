use git2;
use hyper;
use std;
use toml;

error_chain! {
    foreign_links {
        AddrParse(std::net::AddrParseError);
        Git(git2::Error);
        Hyper(hyper::Error);
        Io(std::io::Error);
        ParseInt(std::num::ParseIntError);
        Toml(toml::de::Error);
    }
}
