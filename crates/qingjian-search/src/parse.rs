//! 解析搜索建议的响应体：OpenSearch Suggestions 的 JSON 形状 `["query", ["词1", "词2", …]]`。
//!
//! 只认这一种形状。HTML 错误页、JSONP 包裹、字段被改成对象，都当 [`SearchError::Malformed`]：
//! 联想的失败只记日志，绝不让乱码进候选。

use crate::error::SearchError;

/// 从响应体里取出建议词。形状不对返回 [`SearchError::Malformed`]。
pub fn parse_suggestions(body: &str) -> Result<Vec<String>, SearchError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| SearchError::Malformed)?;
    let array = value.as_array().ok_or(SearchError::Malformed)?;
    // OpenSearch 规范里第二项才是建议；有的实现还会带第三、四项（描述、跳转链接），只取第二项。
    let suggestions = array.get(1).ok_or(SearchError::Malformed)?;
    let items = suggestions.as_array().ok_or(SearchError::Malformed)?;
    // 数组里非字符串的项跳过：一个坏项不该把整批建议丢掉。
    Ok(items
        .iter()
        .filter_map(|item| item.as_str())
        .map(str::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_suggestion_array() {
        let body = r#"["yisitalu",["伊斯塔露","伊斯塔露是谁"]]"#;
        assert_eq!(
            parse_suggestions(body).unwrap(),
            ["伊斯塔露", "伊斯塔露是谁"]
        );
    }

    #[test]
    fn empty_suggestions_are_fine() {
        assert!(parse_suggestions(r#"["yisitalu",[]]"#).unwrap().is_empty());
    }

    #[test]
    fn extra_arrays_after_the_first_are_ignored() {
        let body = r#"["q",["甲","乙"],["说明"],["https://example.com"]]"#;
        assert_eq!(parse_suggestions(body).unwrap(), ["甲", "乙"]);
    }

    #[test]
    fn non_string_items_are_skipped() {
        let body = r#"["q",["甲",3,null,"乙"]]"#;
        assert_eq!(parse_suggestions(body).unwrap(), ["甲", "乙"]);
    }

    #[test]
    fn wraps_and_wrong_shapes_are_rejected() {
        assert!(parse_suggestions("<html>not json</html>").is_err());
        assert!(parse_suggestions("callback([\"q\",[]])").is_err());
        assert!(parse_suggestions(r#"["q"]"#).is_err());
        assert!(parse_suggestions(r#"{"query":"q"}"#).is_err());
        assert!(parse_suggestions("").is_err());
    }
}
