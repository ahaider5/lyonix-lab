use crate::domain::{
    ChatMessage, ChatRequest, ChatResponse, ChatStreamEvent, InferenceError, RuntimeConfiguration,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// Client for the managed llama.cpp server endpoint. All llama.cpp-specific
/// HTTP/endpoint details live behind this type; callers pass runtime-neutral
/// domain requests and receive domain responses.
///
/// The endpoint must be the managed localhost endpoint already produced by the
/// llama.cpp adapter (`http://127.0.0.1:<port>/v1`). Non-localhost targets are
/// rejected here as a second guard alongside configuration validation.
pub struct LlamaCppInferenceClient {
    host: String,
    port: u16,
    path_prefix: String,
}

impl LlamaCppInferenceClient {
    /// Build a client for the managed endpoint from the runtime configuration.
    pub fn from_configuration(config: &RuntimeConfiguration) -> Result<Self, InferenceError> {
        Self::from_endpoint(&format!(
            "http://{}:{}/v1",
            config.network.host, config.network.port
        ))
    }

    /// Build a client from an explicit endpoint string. Only localhost
    /// endpoints are accepted.
    pub fn from_endpoint(endpoint: &str) -> Result<Self, InferenceError> {
        let rest = endpoint
            .strip_prefix("http://")
            .ok_or_else(|| InferenceError::NonLocalhost(endpoint.to_string()))?;
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let path = if path.is_empty() { "/".to_string() } else { format!("/{path}") };
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| InferenceError::NonLocalhost(endpoint.to_string()))?;
        let port = port
            .parse::<u16>()
            .map_err(|_| InferenceError::NonLocalhost(endpoint.to_string()))?;
        let is_local = host == "127.0.0.1" || host == "localhost" || host == "::1";
        if !is_local {
            return Err(InferenceError::NonLocalhost(endpoint.to_string()));
        }
        Ok(Self {
            host: host.to_string(),
            port,
            path_prefix: path.trim_end_matches('/').to_string(),
        })
    }

    /// Blocking chat completion. Sends the request, waits for the full
    /// response, and returns the assembled assistant message.
    pub fn chat(
        &self,
        request: &ChatRequest,
        timeout: Duration,
    ) -> Result<ChatResponse, InferenceError> {
        request.validate()?;
        let started = Instant::now();
        let body = serde_json::to_vec(request)
            .map_err(|e| InferenceError::Serialization(e.to_string()))?;
        let path_string = self.path_prefix.clone() + "/chat/completions";
        let raw = self.post_json(&path_string, &body, timeout)?;
        let value: serde_json::Value = serde_json::from_slice(&raw)
            .map_err(|e| InferenceError::Serialization(e.to_string()))?;
        parse_chat_response(&value, started.elapsed().as_secs_f64() * 1000.0)
    }

    /// Blocking streaming chat completion. Calls `on_event` for each delta and
    /// a final `Done` event; returns when the stream ends.
    pub fn chat_stream<F>(
        &self,
        request: &ChatRequest,
        timeout: Duration,
        mut on_event: F,
    ) -> Result<(), InferenceError>
    where
        F: FnMut(ChatStreamEvent),
    {
        request.validate()?;
        let mut streamed: serde_json::Value =
            serde_json::to_value(request).map_err(|e| InferenceError::Serialization(e.to_string()))?;
        streamed["stream"] = serde_json::Value::Bool(true);
        let body = serde_json::to_vec(&streamed)
            .map_err(|e| InferenceError::Serialization(e.to_string()))?;
        let mut stream = self.connect(timeout)?;
        let head = format!(
            "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\nAccept: text/event-stream\r\n\r\n",
            path = self.path_prefix.clone() + "/chat/completions",
            host = self.host,
            port = self.port,
            len = body.len(),
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(&body)?;
        stream.flush()?;
        let mut reader = BufReader::new(stream);
        let status = read_status_line(&mut reader)?;
        if status != 200 {
            let message = read_error_body(&mut reader);
            return Err(InferenceError::Status { code: status, message });
        }
        // Consume response headers before the SSE body (SSE arrives over
        // HTTP/1.1 chunked transfer; the raw data lines follow the blank line).
        let _headers = read_headers_until_blank(&mut reader)?;
        // SSE state machine: each `data: {json}` line is one chunk; a final
        // `data: [DONE]` terminates the stream.
        let mut done = false;
        loop {
            let line = read_line(&mut reader)?;
            let Some(data) = line.strip_prefix("data: ") else { continue };
            if data.trim() == "[DONE]" {
                if !done {
                    on_event(ChatStreamEvent::Done { finish_reason: None });
                }
                break;
            }
            let value: serde_json::Value = serde_json::from_str(data)
                .map_err(|e| InferenceError::Serialization(e.to_string()))?;
            match parse_stream_chunk(&value) {
                Some(event) => {
                    if matches!(event, ChatStreamEvent::Done { .. }) {
                        done = true;
                    }
                    on_event(event);
                }
                None => continue,
            }
        }
        Ok(())
    }

    fn connect(&self, timeout: Duration) -> Result<TcpStream, InferenceError> {
        let stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| InferenceError::Http(e.to_string()))?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        Ok(stream)
    }

    /// POST JSON and read the full (possibly chunked) response body.
    fn post_json(&self, path: &str, body: &[u8], timeout: Duration) -> Result<Vec<u8>, InferenceError> {
        let mut stream = self.connect(timeout)?;
        let head = format!(
            "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
            host = self.host,
            port = self.port,
            len = body.len(),
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(body)?;
        stream.flush()?;
        let mut reader = BufReader::new(stream);
        let status = read_status_line(&mut reader)?;
        let chunked = read_headers_until_blank(&mut reader)?
            .iter()
            .any(|h| h.to_ascii_lowercase().contains("transfer-encoding:") && h.to_ascii_lowercase().contains("chunked"));
        let raw = if chunked {
            decode_chunked(&mut reader)?
        } else {
            read_to_end(&mut reader)?
        };
        if status != 200 {
            let message = String::from_utf8_lossy(&raw).to_string();
            return Err(InferenceError::Status { code: status, message });
        }
        Ok(raw)
    }
}

fn read_status_line(reader: &mut BufReader<TcpStream>) -> Result<u16, InferenceError> {
    let line = read_line(reader)?;
    let code = line
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<u16>().ok())
        .ok_or_else(|| InferenceError::Http(format!("malformed status line: {line}")))?;
    Ok(code)
}

fn read_headers_until_blank(reader: &mut BufReader<TcpStream>) -> Result<Vec<String>, InferenceError> {
    let mut headers = Vec::new();
    loop {
        let line = read_line(reader)?;
        if line.is_empty() {
            return Ok(headers);
        }
        headers.push(line);
    }
}

fn read_error_body(reader: &mut BufReader<TcpStream>) -> String {
    let mut body = String::new();
    let _ = reader.read_to_string(&mut body);
    body
}

fn read_to_end(reader: &mut BufReader<TcpStream>) -> Result<Vec<u8>, InferenceError> {
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| InferenceError::Http(e.to_string()))?;
    Ok(buf)
}

fn read_line(reader: &mut impl BufRead) -> Result<String, InferenceError> {
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| InferenceError::Http(e.to_string()))?;
    while line.ends_with('\n') || line.ends_with('\r') {
        line.pop();
    }
    Ok(line)
}

/// Decode an HTTP/1.1 chunked transfer body into raw bytes.
fn decode_chunked(reader: &mut BufReader<TcpStream>) -> Result<Vec<u8>, InferenceError> {
    let mut out = Vec::new();
    loop {
        let size_line = read_line(reader)?;
        let size_str = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16)
            .map_err(|_| InferenceError::Http(format!("bad chunk size: {size_line}")))?;
        if size == 0 {
            break;
        }
        let mut chunk = vec![0_u8; size];
        reader
            .read_exact(&mut chunk)
            .map_err(|e| InferenceError::Http(e.to_string()))?;
        out.extend_from_slice(&chunk);
        // Trailing CRLF after each chunk.
        let _ = read_line(reader)?;
    }
    Ok(out)
}

fn parse_chat_response(value: &serde_json::Value, latency_ms: f64) -> Result<ChatResponse, InferenceError> {
    let choice = value
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| InferenceError::Serialization("response has no choices[0]".into()))?;
    let content = choice
        .pointer("/message/content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| InferenceError::Serialization("choices[0].message.content missing".into()))?
        .to_string();
    let finish_reason = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let usage = value.get("usage");
    Ok(ChatResponse {
        content,
        model: value.get("model").and_then(|v| v.as_str()).map(|s| s.to_string()),
        finish_reason,
        prompt_tokens: usage.and_then(|u| u.get("prompt_tokens")).and_then(|v| v.as_u64()),
        completion_tokens: usage.and_then(|u| u.get("completion_tokens")).and_then(|v| v.as_u64()),
        total_latency_ms: Some(latency_ms),
    })
}

fn parse_stream_chunk(value: &serde_json::Value) -> Option<ChatStreamEvent> {
    let choice = value.get("choices")?.get(0)?;
    let finish_reason = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let delta_content = choice
        .pointer("/delta/content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if finish_reason.is_some() {
        return Some(ChatStreamEvent::Done { finish_reason });
    }
    if !delta_content.is_empty() {
        return Some(ChatStreamEvent::Delta { content: delta_content.to_string() });
    }
    None
}

/// Convenience: build a single-turn chat request.
pub fn single_turn_request(prompt: &str, max_tokens: u32) -> ChatRequest {
    ChatRequest {
        model: None,
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
        temperature: None,
        top_p: None,
        top_k: None,
        max_tokens: Some(max_tokens),
        stream: false,
    }
}
