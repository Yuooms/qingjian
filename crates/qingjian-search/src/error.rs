use thiserror::Error;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("failed to start async runtime: {0}")]
    Runtime(#[from] std::io::Error),

    #[error("suggestion request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("suggestion response is larger than {0} bytes")]
    TooLarge(usize),

    /// 响应不是搜索建议的 JSON（HTML 错误页、JSONP 包裹、接口地址写错）。
    #[error("suggestion response is not in the expected format")]
    Malformed,
}
