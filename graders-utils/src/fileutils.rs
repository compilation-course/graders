use lazy_static::lazy_static;
use std::ffi::OsStr;
use std::path::{Component, Path};

lazy_static! {
    static ref DOTDOT: Component<'static> = Component::Normal(OsStr::new(".."));
}

pub fn contains_dotdot<P: AsRef<Path>>(p: P) -> bool {
    p.as_ref().components().any(|c| c == *DOTDOT)
}
