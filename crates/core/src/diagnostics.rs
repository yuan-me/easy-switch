use crate::{protocol, runtime, *};
use anyhow::{Context, ensure};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
fn row(name: &str, state: &str, detail: impl Into<String>) -> Diagnostic {
    Diagnostic {
        name: name.into(),
        state: state.into(),
        detail: detail.into(),
    }
}
pub async fn models(p: &Provider, ct: &CancellationToken) -> Result<Vec<String>> {
    validate_url(&p.base_url)?;
    let key = crypto::unprotect(p.protected_key.as_deref().context("API Key 未配置")?)?;
    let mut req = runtime::client()?
        .get(format!("{}/models", p.base_url.trim_end_matches('/')))
        .bearer_auth(key)
        .timeout(Duration::from_secs(20));
    for (k, v) in &p.headers {
        req = req.header(k, v);
    }
    let response = tokio::select! {_=ct.cancelled()=>anyhow::bail!("操作已取消"),r=req.send()=>r?};
    ensure!(
        response.status().is_success(),
        "模型列表 HTTP {}",
        response.status().as_u16()
    );
    let body = response.bytes().await?;
    ensure!(body.len() <= 4_000_000, "模型列表过大");
    let v: Value = serde_json::from_slice(&body)?;
    let mut names = v["data"]
        .as_array()
        .context("返回格式不是模型列表")?
        .iter()
        .filter_map(|m| m["id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}
pub async fn doctor(
    p: &Provider,
    all: &[Provider],
    network: bool,
    ct: &CancellationToken,
) -> Result<Vec<Diagnostic>> {
    let mut rows = vec![];
    match p.validate(all) {
        Ok(()) => rows.push(row("本地配置", "passed", "必填字段、地址和上下文设置有效")),
        Err(e) => {
            rows.push(row("本地配置", "failed", e.to_string()));
            return Ok(rows);
        }
    }
    if p.mode == AuthMode::Official {
        rows.push(row("官方身份", "untested", "由 Codex 官方登录流程确认"));
        return Ok(rows);
    }
    rows.push(row(
        "图片兼容配置",
        if p.image_compatibility {
            "passed"
        } else {
            "untested"
        },
        if p.image_compatibility {
            "已选择 Sub2API 客户端执行器兼容；切换后需完整重启并新建会话"
        } else {
            "未开启；仅有上游作图权限不代表客户端工具已注册"
        },
    ));
    rows.push(row(
        "客户端图片工具注册",
        "untested",
        "需在目标 Codex 的新会话确认 image_gen 工具可用",
    ));
    rows.push(row(
        "实际图片返回",
        "untested",
        "需由 Codex 实际调用并显示图片，配置检查不能代替此项",
    ));
    if p.protocol == Protocol::ChatCompletions {
        rows.push(row(
            "原生工具与压缩",
            "warning",
            "Chat 转换仅支持 function 工具，不支持原生图片工具及 Responses compact",
        ));
    }
    if !network {
        return Ok(rows);
    }
    if p.mode == AuthMode::Aggregate {
        rows.push(row("聚合成员", "untested", "请对各成员分别执行联网诊断"));
        return Ok(rows);
    }
    match models(p, ct).await {
        Ok(names) => rows.push(row(
            "模型列表",
            if names.contains(&p.model) {
                "passed"
            } else {
                "warning"
            },
            format!(
                "返回 {} 个模型；{}",
                names.len(),
                if names.contains(&p.model) {
                    "包含当前模型"
                } else {
                    "未列出当前模型，仍可手动测试"
                }
            ),
        )),
        Err(e) => rows.push(row("模型列表", "failed", runtime::safe_error(&e))),
    }
    let client = runtime::client()?;
    for (name, stream, tool, compact) in [
        ("文本请求", false, false, false),
        ("流式请求", true, false, false),
        ("函数工具", false, true, false),
        ("Responses 压缩", false, false, true),
    ] {
        if ct.is_cancelled() {
            rows.push(row(name, "warning", "已取消"));
            break;
        }
        if compact && p.protocol == Protocol::ChatCompletions {
            continue;
        }
        let mut payload = json!({"model":p.model,"input":"Reply with the single word OK.","stream":stream,"max_output_tokens":128});
        if compact {
            payload = json!({"model":p.model,"input":[{"role":"user","content":"Remember the word OK."},{"role":"assistant","content":"OK"}]});
        }
        if tool {
            payload["tools"] = json!([{"type":"function","name":"easy_switch_probe","description":"Return OK. Diagnostic only; this tool is never executed.","parameters":{"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false},"strict":true}]);
            payload["tool_choice"] = json!({"type":"function","name":"easy_switch_probe"});
        }
        let endpoint = if p.protocol == Protocol::ChatCompletions {
            payload = protocol::to_chat(&payload, &p.model)?;
            "chat/completions"
        } else if compact {
            "responses/compact"
        } else {
            "responses"
        };
        let start = Instant::now();
        let result = tokio::select! {_=ct.cancelled()=>Err(anyhow::anyhow!("已取消")),r=async{let response=runtime::request(&client,p,endpoint)?.timeout(Duration::from_secs(45)).json(&payload).send().await?;ensure!(response.status().is_success(),"HTTP {}",response.status().as_u16());let data=response.bytes().await?;ensure!(data.len()<4_000_000,"诊断响应过大");if stream{let text=std::str::from_utf8(&data)?;ensure!(text.contains("data:")&&(text.contains("response.completed")||text.contains("[DONE]")),"未收到完整 SSE 结束事件");}else{let v:Value=serde_json::from_slice(&data)?;if tool{let calls=if p.protocol==Protocol::ChatCompletions{v.pointer("/choices/0/message/tool_calls")}else{v.get("output")};let matched=calls.and_then(Value::as_array).is_some_and(|items|items.iter().any(|x|x["name"]=="easy_switch_probe"||x["function"]["name"]=="easy_switch_probe"));ensure!(matched,"未返回预期诊断工具调用");}else if compact{ensure!(v["output"].is_array(),"不是有效压缩结果");}else{ensure!(v["output"].is_array()||v["choices"].is_array(),"不是有效文本结果");}}Ok::<_,anyhow::Error>(())}=>r};
        match result {
            Ok(()) => rows.push(row(
                name,
                "passed",
                format!("{} ms；仅使用合成测试内容", start.elapsed().as_millis()),
            )),
            Err(e) => rows.push(row(name, "failed", runtime::safe_error(&e))),
        }
    }
    Ok(rows)
}
