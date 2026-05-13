//! NexusCore Day 35 — ReAct Agent WASI Component
//!
//! Memory layout (4 MB Wasm linear memory):
//!   [0x000000 - 0x0FFFFF]  Stack (1 MB, managed by Wasm runtime)
//!   [0x100000 - 0x1FFFFF]  Scratchpad arena (bump alloc, reset per run)
//!   [0x200000 - 0x3FFFFF]  HTTP response buffer (2 MB)
//!
//! No heap allocator (no dlmalloc / wee_alloc). All dynamic data goes through
//! the arena. This eliminates TLB pressure from per-step heap fragmentation.

#![no_main]

mod bindings;

use std::sync::atomic::{AtomicUsize, Ordering};

use bindings::exports::nexuscore::agent::react_agent::{Guest, StepResult};

// ── Bump Arena ───────────────────────────────────────────────────────────────
// We carve out a 1 MB scratchpad at a fixed linear memory offset.
// In a real WASI P2 component the host can pass a resource handle to shared
// memory; here we use a static offset into our own linear memory.
const ARENA_BASE: usize = 0x100_000;
const ARENA_SIZE: usize = 0x100_000; // 1 MB
static ARENA_PTR: AtomicUsize = AtomicUsize::new(ARENA_BASE);

fn arena_alloc(size: usize) -> *mut u8 {
    let align = 8;
    let current = ARENA_PTR.load(Ordering::Relaxed);
    let aligned = (current + align - 1) & !(align - 1);
    let next = aligned + size;
    if next > ARENA_BASE + ARENA_SIZE {
        panic!("Arena OOM: requested {size} bytes, arena exhausted");
    }
    ARENA_PTR.store(next, Ordering::Relaxed);
    aligned as *mut u8
}

fn arena_reset() {
    ARENA_PTR.store(ARENA_BASE, Ordering::Relaxed);
}

/// Arena-backed String wrapper — avoids Box<str> / String heap alloc.
struct ArenaStr {
    ptr: *const u8,
    len: usize,
}
impl ArenaStr {
    fn from_str(s: &str) -> Self {
        let ptr = arena_alloc(s.len());
        unsafe { std::ptr::copy_nonoverlapping(s.as_ptr(), ptr, s.len()); }
        ArenaStr { ptr, len: s.len() }
    }
    fn as_str(&self) -> &str {
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.ptr, self.len)) }
    }
}

// ── ReAct Prompt Builder ─────────────────────────────────────────────────────
fn build_prompt(query: &str, history: &[StepResult]) -> String {
    let mut prompt = String::from(
        "You are a ReAct agent. Respond in this exact format:\n\
         Thought: <your reasoning>\n\
         Action: Search[<search query>]\n\
         OR\n\
         Thought: <your reasoning>\n\
         Final Answer: <your answer>\n\n\
         Available tools: Search[query] — searches the web.\n\n"
    );
    prompt.push_str(&format!("Question: {query}\n\n"));
    for step in history {
        prompt.push_str(&format!("Thought: {}\n", step.thought));
        if let (Some(action), Some(input)) = (&step.action, &step.action_input) {
            prompt.push_str(&format!("{action}[{input}]\n"));
        }
        if let Some(obs) = &step.observation {
            prompt.push_str(&format!("Observation: {obs}\n"));
        }
    }
    prompt
}
// ── Simple HTTP POST via wasi:http ───────────────────────────────────────────
fn http_post(url: &str, body: &str, headers: &[(&str, &str)]) -> Result<String, String> {
    use wasi::http::outgoing_handler;
    use wasi::http::types::{Headers, Method, OutgoingBody, OutgoingRequest, RequestOptions, Scheme};

    let req_headers = Headers::new();
    for (k, v) in headers {
        req_headers
            .append(&k.to_string(), &v.as_bytes().to_vec())
            .map_err(|e| format!("Header error: {e:?}"))?;
    }

    let req = OutgoingRequest::new(req_headers);
    req.set_method(&Method::Post).map_err(|_| "set method failed")?;

    // Parse URL into scheme + authority + path
    let (scheme, rest) = if url.starts_with("https://") {
        (Scheme::Https, &url[8..])
    } else if url.starts_with("http://") {
        (Scheme::Http, &url[7..])
    } else {
        return Err(format!("Unsupported scheme in URL: {url}"));
    };

    let (authority, path) = rest.split_once('/').map(|(a, p)| (a, format!("/{p}")))
        .unwrap_or((rest, "/".to_string()));

    req.set_scheme(Some(&scheme)).map_err(|_| "set scheme failed")?;
    req.set_authority(Some(authority)).map_err(|_| "set authority failed")?;
    req.set_path_with_query(Some(&path)).map_err(|_| "set path failed")?;

    // Write body
    let outgoing_body = req.body().map_err(|_| "get body failed")?;
    {
        let stream = outgoing_body.write().map_err(|_| "get stream failed")?;
        let body_bytes = body.as_bytes();
        stream
            .blocking_write_and_flush(body_bytes)
            .map_err(|e| format!("write body: {e:?}"))?;
    }
    OutgoingBody::finish(outgoing_body, None).map_err(|_| "finish body failed")?;

    let opts = RequestOptions::new();
    opts.set_connect_timeout(Some(30_000_000_000u64)).ok(); // 30s in ns
    opts.set_first_byte_timeout(Some(60_000_000_000u64)).ok();

    let future_response = outgoing_handler::handle(req, Some(opts))
        .map_err(|e| format!("handle failed: {e:?}"))?;

    // Must wait on the WASI pollable — spinning on `get()` never yields to the host, so the
    // outbound Hyper request never completes (appears "stuck" at step 0 forever).
    future_response.subscribe().block();

    let response = match future_response.get() {
        Some(Ok(Ok(resp))) => resp,
        Some(Ok(Err(e))) => return Err(format!("HTTP error: {e:?}")),
        Some(Err(_)) => return Err("Response already consumed".to_string()),
        None => return Err("HTTP future still pending after pollable.block()".to_string()),
    };

    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }

    let body = response.consume().map_err(|_| "consume body failed")?;
    let stream = body.stream().map_err(|_| "body stream failed")?;
    let mut data = Vec::new();
    loop {
        let chunk = match stream.blocking_read(16_384) {
            Ok(c) => c,
            Err(_) => break,
        };
        if chunk.is_empty() {
            break;
        }
        data.extend_from_slice(&chunk);
    }
    String::from_utf8(data).map_err(|e| format!("UTF-8 decode: {e}"))
}

// ── LLM Call ─────────────────────────────────────────────────────────────────
fn call_llm(
    prompt: &str,
    endpoint: &str,
    api_key: &str,
    react_step: u32,
) -> Result<String, String> {
    // Minimal JSON request body; response parsed with serde_json.
    let body = format!(
        r#"{{"model":"claude-sonnet-4-20250514","max_tokens":512,"nexuscore_step":{},"messages":[{{"role":"user","content":{}}}]}}"#,
        react_step,
        json_string_escape(prompt)
    );
    let response_json = http_post(
        endpoint,
        &body,
        &[
            ("content-type", "application/json"),
            ("x-api-key", api_key),
            ("anthropic-version", "2023-06-01"),
        ],
    )?;
    // Extract text from: {"content":[{"type":"text","text":"..."}],...}
    extract_json_text_field(&response_json)
}

// ── Search Call ───────────────────────────────────────────────────────────────
fn call_search(query: &str, endpoint: &str) -> Result<String, String> {
    let body = format!(r#"{{"q":{}}}"#, json_string_escape(query));
    let raw = http_post(
        &format!("{endpoint}/search"),
        &body,
        &[("content-type", "application/json")],
    )?;
    // Extract first result snippet
    extract_first_snippet(&raw)
}

// ── Minimal JSON helpers (no serde dependency) ────────────────────────────────
fn json_string_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"'  => out.push_str(r#"\""#),
            '\\' => out.push_str(r"\\"),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '\t' => out.push_str(r"\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!(r"\u{:04x}", c as u32)),
            c    => out.push(c),
        }
    }
    out.push('"');
    out
}

fn extract_json_text_field(json: &str) -> Result<String, String> {
    // Anthropic-style message: { "content": [ { "type": "text", "text": "..." } ], ... }
    // Parse with serde_json so spacing, key order, and escapes match Python json.dumps / real APIs.
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        format!("LLM response is not valid JSON ({e}): {}", json.chars().take(200).collect::<String>())
    })?;
    let content = v
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| format!("missing content array in LLM response: {json}"))?;

    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                return Ok(t.to_string());
            }
        }
    }
    for block in content {
        if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
            return Ok(t.to_string());
        }
    }
    Err(format!("No assistant text in content[]: {json}"))
}

fn extract_first_snippet(json: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or(serde_json::Value::Null);
    if let Some(arr) = v.get("results").and_then(|r| r.as_array()) {
        if let Some(first) = arr.first() {
            if let Some(s) = first.get("snippet").and_then(|x| x.as_str()) {
                return Ok(s.to_string());
            }
        }
    }
    Ok(json.chars().take(512).collect())
}
// ── ReAct Parser ─────────────────────────────────────────────────────────────
struct ParsedResponse {
    thought: String,
    action: Option<String>,
    action_input: Option<String>,
    final_answer: Option<String>,
}

fn parse_react_response(text: &str) -> ParsedResponse {
    let mut thought = String::new();
    let mut action: Option<String> = None;
    let mut action_input: Option<String> = None;
    let mut final_answer: Option<String> = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Thought:") {
            thought = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("Action:") {
            let act = rest.trim();
            // Parse: Search[query]
            if let (Some(open), Some(close)) = (act.find('['), act.rfind(']')) {
                action = Some(act[..open].trim().to_string());
                action_input = Some(act[open+1..close].trim().to_string());
            } else {
                action = Some(act.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("Final Answer:") {
            final_answer = Some(rest.trim().to_string());
        }
    }

    if thought.is_empty() {
        thought = text.lines().next().unwrap_or("...").trim().to_string();
    }

    ParsedResponse { thought, action, action_input, final_answer }
}

// ── WIT Export Implementation ─────────────────────────────────────────────────
struct AgentImpl;

impl Guest for AgentImpl {
    fn run_step(
        query: String,
        history: Vec<StepResult>,
        llm_endpoint: String,
        search_endpoint: String,
        api_key: String,
    ) -> Result<StepResult, String> {
        let step_index = history.len() as u32;

        // Build prompt in arena-backed scratch (ArenaStr for sub-components)
        let _prompt_key = ArenaStr::from_str(&format!("step-{step_index}"));
        let prompt = build_prompt(&query, &history);

        // Call LLM
        let llm_text = call_llm(&prompt, &llm_endpoint, &api_key, step_index)?;
        let parsed = parse_react_response(&llm_text);

        // If action is Search, execute it
        let observation = if parsed.action.as_deref() == Some("Search") {
            if let Some(ref q) = parsed.action_input {
                Some(call_search(q, &search_endpoint)?)
            } else {
                None
            }
        } else {
            None
        };

        Ok(StepResult {
            thought: parsed.thought,
            action: parsed.action,
            action_input: parsed.action_input,
            observation,
            final_answer: parsed.final_answer,
            step_index,
        })
    }

    fn reset_arena() {
        arena_reset();
    }
}

bindings::export!(AgentImpl with_types_in bindings);
