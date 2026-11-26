pub(crate) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
