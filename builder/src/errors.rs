error_chain! {
    errors {
        RunError(s: String) {
            description("test run error")
            display("fatal error when running test: {}", s)
        }
        UnknownRepositorySource(s: String) {
            description("unknown repository source")
            display("unknown repository source {}", s)
        }
        ZipExtractError {
            description("zip extract error")
            display("cannot extract zip file")
        }
    }
}
