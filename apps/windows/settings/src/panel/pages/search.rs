//! 「搜索联想」页：`[search]` 各项与「测试联通」（后台线程跑）。
//!
//! 与「云服务」页的云联想是两套独立的数据源，可以一起开：两路结果由 Core 合并后补进候选末尾。

use std::time::Duration;

use qingjian_search::{SearchConfig, SearchProbe};
use windows_reactor::*;

use crate::panel::controls::{field, labeled, note, page};
use crate::panel::probe_status::ProbeStatus;
use crate::panel::{Message, Settings};

/// 后台跑一次连通性测试，轮询到有结果或被取消。
pub(crate) fn run_test(
    config: &SearchConfig,
    cancel: &CancellationToken,
) -> Result<String, String> {
    let probe = SearchProbe::start(config).map_err(|error| error.to_string())?;
    loop {
        if cancel.is_cancelled() {
            return Err("已取消".to_owned());
        }
        if let Some(result) = probe.poll() {
            return result
                .map(|report| {
                    let preview = if report.samples.is_empty() {
                        "没有返回建议".to_owned()
                    } else {
                        format!("如「{}」", report.samples.join("」「"))
                    };
                    format!(
                        "连接成功：{} → {} 条建议，耗时 {} ms（{preview}）",
                        report.letters,
                        report.count,
                        report.elapsed.as_millis()
                    )
                })
                .map_err(|error| error.to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(crate) fn view(settings: &Settings, context: &mut ViewContext<Settings>) -> View {
    let s = &settings.config.search;
    let status = match &settings.search_status {
        ProbeStatus::Idle => String::new(),
        ProbeStatus::Testing => "测试中…".to_owned(),
        ProbeStatus::Ok(message) => message.clone(),
        ProbeStatus::Failed(message) => format!("失败：{message}"),
    };
    let rows = [
        field(
            "启用搜索联想",
            "开启后组句时把已敲的拼音字母发给下面的搜索建议接口，取回的中文联想词补进候选末尾——词库里没有的人名、地名、新词也能打出来。关着时完全不离线。",
            ToggleSwitch::new()
                .is_on(s.enabled)
                .on_toggled(context.callback(Message::SearchEnabled)),
        ),
        field(
            "联想词格数",
            "搜到的词只补进第一页末尾这几格，前面的本地候选不动；没搜到就什么都不变。想多留格子给本地候选就调小。",
            NumberBox::new()
                .minimum(1.0)
                .maximum(9.0)
                .value(s.slots as f64)
                .on_value_changed(context.callback(Message::SearchSlots)),
        ),
        field(
            "接口地址",
            "要求 OpenSearch Suggestions 格式（一个 JSON 数组：`[\"拼音\", [\"联想词\", …]]`），缺省用 Bing 的公开接口，不用密钥。",
            TextBox::new()
                .text(s.endpoint.clone())
                .on_text_changed(context.callback(Message::SearchEndpoint)),
        ),
        field(
            "市场",
            "接口的 `mkt` 参数，决定联想词的语言与地区；简体中文用 zh-CN，中国香港用 zh-HK，中国台湾用 zh-TW。",
            TextBox::new()
                .text(s.market.clone())
                .on_text_changed(context.callback(Message::SearchMarket)),
        ),
        field(
            "最少字母数",
            "敲够这么多个拼音字母才发请求；单字母查询没有意义，也省一次往返。",
            NumberBox::new()
                .minimum(1.0)
                .maximum(8.0)
                .value(s.min_letters as f64)
                .on_value_changed(context.callback(Message::SearchMinLetters)),
        ),
        field(
            "停键延迟",
            "停止敲键多久之后才真正发请求（毫秒）。打字过程中不查，省流量也不让候选窗口乱跳。",
            NumberBox::new()
                .minimum(0.0)
                .maximum(2000.0)
                .value(s.debounce_ms as f64)
                .on_value_changed(context.callback(Message::SearchDebounce)),
        ),
        field(
            "请求超时",
            "单次请求超时（毫秒），超时就丢掉这次结果，不阻塞打字。",
            NumberBox::new()
                .minimum(200.0)
                .maximum(10000.0)
                .value(s.timeout_ms as f64)
                .on_value_changed(context.callback(Message::SearchTimeout)),
        ),
        labeled(
            "",
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(12.0)
                .children((
                    Button::new()
                        .on_click(context.message(Message::TestSearch))
                        .content("测试联通"),
                    TextBlock::new().text(status),
                )),
        ),
        note(
            "用上面填的地址与市场查一次固定拼音（zhongguo）。走不通时先查这里——接口地址写错、没有网络、被代理或防火墙挡住都会失败；Server 进程看不到终端里的代理变量。",
        ),
    ];
    page("搜索联想", StackPanel::new().spacing(16.0).children(rows))
}
