//! 搜索建议接口的 HTTP 客户端：拼 URL、发 GET、限量读响应体、解析。

use std::time::Duration;

use crate::config::SearchConfig;
use crate::error::SearchError;
use crate::parse::parse_suggestions;

/// 响应体上限。建议列表只有几 KB，超过这个量级说明拿到的不是建议（错误页、被劫持的页面）。
const MAX_BODY_BYTES: usize = 256 * 1024;

/// 一次搜索建议查询。
pub struct SuggestClient {
    /// 底层客户端，超时与 User-Agent 在建它时定好。
    client: reqwest::Client,

    /// 接口地址。
    endpoint: String,

    /// 市场（`mkt`）。
    market: String,

    /// 界面语言（`setlang`）。
    setlang: String,
}

impl SuggestClient {
    pub fn new(config: &SearchConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .user_agent(concat!("qingjian/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self {
            client,
            endpoint: config.endpoint.trim().to_owned(),
            market: config.market.trim().to_owned(),
            setlang: config.setlang.trim().to_owned(),
        }
    }

    /// 市场名，缓存键要用（同一拼音在不同市场结果不同）。
    pub fn market(&self) -> &str {
        &self.market
    }

    /// 查一次拼音字母对应的建议词，返回原始建议（还没按拼音筛选）。
    pub async fn suggest(&self, letters: &str) -> Result<Vec<String>, SearchError> {
        let mut url = reqwest::Url::parse(&self.endpoint).map_err(|_| SearchError::Malformed)?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("query", letters);
            query.append_pair("mkt", &self.market);
            query.append_pair("setlang", &self.setlang);
        }
        let response = self.client.get(url).send().await?.error_for_status()?;
        let bytes = response.bytes().await?;
        if bytes.len() > MAX_BODY_BYTES {
            return Err(SearchError::TooLarge(MAX_BODY_BYTES));
        }
        // 建议是 UTF-8；个别接口会带点杂字节，坏字节换掉而不是整条丢弃
        parse_suggestions(&String::from_utf8_lossy(&bytes))
    }
}
