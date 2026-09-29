use crate::Result;
use anyhow::{bail, ensure};
use serde_json::{Value, json};

pub fn to_chat(input: &Value, model: &str) -> Result<Value> {
    ensure!(
        input.get("previous_response_id").is_none_or(Value::is_null),
        "Chat 转换需要完整历史，不支持 previous_response_id"
    );
    let mut messages = vec![];
    if let Some(instructions) = input.get("instructions") {
        messages.push(json!({"role":"system","content":instructions}));
    }
    if let Some(text) = input["input"].as_str() {
        messages.push(json!({"role":"user","content":text}));
    } else if let Some(items) = input["input"].as_array() {
        for item in items {
            ensure!(
                item.get("encrypted_content").is_none(),
                "加密历史绑定原供应商，不能转换"
            );
            match item["type"].as_str().unwrap_or("message") {
                "message" => {
                    let mut parts = vec![];
                    if let Some(text) = item["content"].as_str() {
                        parts.push(json!({"type":"text","text":text}));
                    } else if let Some(content) = item["content"].as_array() {
                        for part in content {
                            parts.push(match part["type"].as_str(){Some("input_text"|"output_text"|"text")=>json!({"type":"text","text":part["text"]}),Some("input_image")=>json!({"type":"image_url","image_url":{"url":part["image_url"]}}),_=>bail!("Chat 转换不支持该内容类型")});
                        }
                    }
                    messages.push(json!({"role":if item["role"]=="developer"{json!("system")}else{item["role"].clone()},"content":parts}));
                }
                "function_call" => {
                    let call = json!({"id":item["call_id"],"type":"function","function":{"name":item["name"],"arguments":item["arguments"]}});
                    if let Some(last) = messages
                        .last_mut()
                        .filter(|m| m["role"] == "assistant" && m["tool_calls"].is_array())
                    {
                        last["tool_calls"].as_array_mut().unwrap().push(call);
                    } else {
                        messages
                            .push(json!({"role":"assistant","content":null,"tool_calls":[call]}));
                    }
                }
                "function_call_output" => {
                    messages.push(json!({"role":"tool","tool_call_id":item["call_id"],"content":item["output"].as_str().map(str::to_owned).unwrap_or(item["output"].to_string())}));
                }
                "reasoning" => {}
                _ => bail!("Chat 转换不支持该工具或输入类型，请改用 Responses"),
            }
        }
    }
    let mut out = json!({"model":model,"messages":messages,"stream":input["stream"].as_bool().unwrap_or(false)});
    for key in ["temperature", "top_p", "parallel_tool_calls"] {
        if let Some(v) = input.get(key) {
            out[key] = v.clone();
        }
    }
    if let Some(v) = input.get("max_output_tokens") {
        out["max_tokens"] = v.clone();
    }
    if let Some(format) = input.pointer("/text/format") {
        out["response_format"] = format.clone();
    }
    if let Some(tools) = input["tools"].as_array() {
        let mut mapped = vec![];
        for tool in tools {
            ensure!(
                tool["type"] == "function",
                "Chat 转换仅支持 function 工具；不支持原生图片、搜索或自定义工具"
            );
            let mut f = tool.clone();
            f.as_object_mut().unwrap().remove("type");
            mapped.push(json!({"type":"function","function":f}));
        }
        out["tools"] = json!(mapped);
    }
    if let Some(choice) = input.get("tool_choice") {
        out["tool_choice"] = if choice.is_object() {
            ensure!(choice["type"] == "function", "不支持该工具选择方式");
            json!({"type":"function","function":{"name":choice["name"]}})
        } else {
            choice.clone()
        };
    }
    if out["stream"] == true {
        out["stream_options"] = json!({"include_usage":true});
    }
    Ok(out)
}
fn usage(v: &Value) -> Value {
    json!({"input_tokens":v["prompt_tokens"],"output_tokens":v["completion_tokens"],"total_tokens":v["total_tokens"],"input_tokens_details":{"cached_tokens":v["prompt_tokens_details"]["cached_tokens"]},"output_tokens_details":{"reasoning_tokens":v["completion_tokens_details"]["reasoning_tokens"]}})
}
pub fn from_chat(v: &Value, model: &str) -> Result<Value> {
    let m = &v["choices"][0]["message"];
    ensure!(m.is_object(), "上游未返回 Chat 消息");
    let mut output = vec![];
    if let Some(text) = m["content"].as_str() {
        output.push(json!({"id":format!("msg_{}",uuid::Uuid::new_v4()),"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":text,"annotations":[]}]}));
    }
    if let Some(calls) = m["tool_calls"].as_array() {
        for call in calls {
            output.push(json!({"id":format!("fc_{}",uuid::Uuid::new_v4()),"type":"function_call","status":"completed","call_id":call["id"],"name":call["function"]["name"],"arguments":call["function"]["arguments"]}));
        }
    }
    let incomplete = v["choices"][0]["finish_reason"] == "length";
    Ok(
        json!({"id":format!("resp_{}",uuid::Uuid::new_v4()),"object":"response","created_at":chrono::Utc::now().timestamp(),"model":model,"status":if incomplete{"incomplete"}else{"completed"},"incomplete_details":if incomplete{json!({"reason":"max_output_tokens"})}else{Value::Null},"output":output,"usage":usage(&v["usage"])}),
    )
}

pub struct ChatStream {
    id: String,
    model: String,
    output: Vec<Value>,
    tools: std::collections::BTreeMap<u64, usize>,
    message: Option<usize>,
    usage: Value,
    pub finished: bool,
    incomplete: bool,
    sequence: u64,
}
impl ChatStream {
    pub fn new(model: &str) -> Self {
        Self {
            id: format!("resp_{}", uuid::Uuid::new_v4()),
            model: model.into(),
            output: vec![],
            tools: Default::default(),
            message: None,
            usage: Value::Null,
            finished: false,
            incomplete: false,
            sequence: 0,
        }
    }
    fn event(&mut self, kind: &str, mut data: Value) -> Value {
        data["type"] = json!(kind);
        data["sequence_number"] = json!(self.sequence);
        self.sequence += 1;
        data
    }
    fn response(&self, status: &str) -> Value {
        json!({"id":self.id,"object":"response","created_at":chrono::Utc::now().timestamp(),"model":self.model,"status":status,"output":self.output,"usage":self.usage})
    }
    pub fn start(&mut self) -> Vec<Value> {
        vec![
            self.event(
                "response.created",
                json!({"response":self.response("in_progress")}),
            ),
            self.event(
                "response.in_progress",
                json!({"response":self.response("in_progress")}),
            ),
        ]
    }
    pub fn ingest(&mut self, v: &Value) -> Result<Vec<Value>> {
        ensure!(v.get("error").is_none(), "上游流返回错误");
        if v["usage"].is_object() {
            self.usage = usage(&v["usage"]);
        }
        let delta = &v["choices"][0]["delta"];
        let mut events = vec![];
        if let Some(text) = delta["content"].as_str() {
            let index = if let Some(i) = self.message {
                i
            } else {
                let i = self.output.len();
                self.message = Some(i);
                let item = json!({"id":format!("msg_{}",uuid::Uuid::new_v4()),"type":"message","role":"assistant","status":"in_progress","content":[]});
                self.output.push(item.clone());
                events.push(self.event(
                    "response.output_item.added",
                    json!({"output_index":i,"item":item}),
                ));
                let part = json!({"type":"output_text","text":"","annotations":[]});
                self.output[i]["content"] = json!([part.clone()]);
                events.push(self.event("response.content_part.added",json!({"output_index":i,"content_index":0,"item_id":self.output[i]["id"],"part":part})));
                i
            };
            let current = self.output[index]["content"][0]["text"]
                .as_str()
                .unwrap_or("");
            ensure!(current.len() + text.len() < 8_000_000, "文本流超过缓冲限制");
            self.output[index]["content"][0]["text"] = json!(format!("{current}{text}"));
            events.push(self.event("response.output_text.delta",json!({"output_index":index,"content_index":0,"item_id":self.output[index]["id"],"delta":text})));
        }
        if let Some(calls) = delta["tool_calls"].as_array() {
            for call in calls {
                let idx = call["index"].as_u64().unwrap_or(0);
                let index = if let Some(i) = self.tools.get(&idx) {
                    *i
                } else {
                    ensure!(self.tools.len() < 128, "工具调用数量超过限制");
                    let i = self.output.len();
                    let item = json!({"id":format!("fc_{}",uuid::Uuid::new_v4()),"type":"function_call","status":"in_progress","call_id":call["id"],"name":call["function"]["name"],"arguments":""});
                    self.output.push(item.clone());
                    self.tools.insert(idx, i);
                    events.push(self.event(
                        "response.output_item.added",
                        json!({"output_index":i,"item":item}),
                    ));
                    i
                };
                if let Some(args) = call["function"]["arguments"].as_str() {
                    let current = self.output[index]["arguments"].as_str().unwrap_or("");
                    ensure!(current.len() + args.len() < 4_000_000, "工具参数超过限制");
                    self.output[index]["arguments"] = json!(format!("{current}{args}"));
                    events.push(self.event("response.function_call_arguments.delta",json!({"output_index":index,"item_id":self.output[index]["id"],"delta":args})));
                }
            }
        }
        if let Some(reason) = v["choices"][0]["finish_reason"].as_str() {
            ensure!(
                matches!(reason, "stop" | "tool_calls" | "length"),
                "上游流因不支持的原因结束"
            );
            self.finished = true;
            self.incomplete = reason == "length";
        }
        Ok(events)
    }
    pub fn finish(&mut self) -> Result<Vec<Value>> {
        ensure!(self.finished, "上游流提前断开，未伪造完成事件");
        let mut events = vec![];
        for i in 0..self.output.len() {
            self.output[i]["status"] = json!("completed");
            let item = self.output[i].clone();
            if item["type"] == "message" {
                events.push(self.event("response.output_text.done",json!({"output_index":i,"content_index":0,"item_id":item["id"],"text":item["content"][0]["text"]})));
                events.push(self.event("response.content_part.done",json!({"output_index":i,"content_index":0,"item_id":item["id"],"part":item["content"][0]})));
            } else {
                events.push(self.event(
                    "response.function_call_arguments.done",
                    json!({"output_index":i,"item_id":item["id"],"arguments":item["arguments"]}),
                ));
            }
            events.push(self.event(
                "response.output_item.done",
                json!({"output_index":i,"item":item}),
            ));
        }
        let status = if self.incomplete {
            "incomplete"
        } else {
            "completed"
        };
        events.push(self.event(
            &format!("response.{status}"),
            json!({"response":self.response(status)}),
        ));
        Ok(events)
    }
}
