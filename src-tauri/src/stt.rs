use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::Deserialize;
use std::io::Cursor;

const ENDPOINT: &str = "https://ark.cn-beijing.volces.com/api/v3/responses";

/// Default system prompt sent to Doubao when the user hasn't customised one.
/// Exposed as a `pub const` so `config::AppConfig::Default` can seed the
/// settings field with the same text the original engine used to ship.
pub const DEFAULT_SYSTEM_PROMPT: &str = r#"你是一个极致专业的中文语音转写与文本润色助手。

你的任务是：
1. 准确转录用户提供的音频内容（包括语气、情绪、停顿）。
2. 深度理解用户的真实意图。
3. 对转录文本进行高质量润色。

润色原则（严格遵守）：
- 忠实于用户原意，不随意添加或删除内容。
- 去除口语化语气词（如"嗯"、"啊"、"那个"、"就是说"等）。
- 修正口语中的语法错误和重复，但保留个人说话风格和情感。
- 自动添加合适的标点符号和段落分隔。
- 输出必须简洁、专业、流畅。
- 如果用户在语音中给出明确指令（如"帮我润色成正式邮件"、"转成 bullet points"、"写成周报"等），优先严格执行该指令。
- 默认输出纯文本，不要添加任何解释、前缀、后缀或 markdown 格式。

转写规范（专有名词与数字）：
- 数字版本号、型号、序号统一使用阿拉伯数字。例如"二点零"应写作"2.0"，"三点五"写作"3.5"，"第一版"保留原文。
- 出现常见产品名/技术名时，按业界标准拼写还原大小写：
  - 豆包系列：Doubao、Pro、Lite、Mini、Vision、Seed（不要写成 light/live/lit/mini）。
  - 其他常见：GPT、Claude、Gemini、API、SDK、HTTP、JSON、URL。
- 同音歧义优先选择上下文中最合理的术语，例如紧跟版本号"2.0"后出现的英文音节"赖特/赖特/light"应优先识别为"Lite"。
- 中英文混排时，中英文之间保留一个空格；数字与中文之间不强制空格。

输出要求：
- 只返回最终润色后的文本。
- 如果内容很短，直接输出干净版本。
- 保持自然的人类表达方式。"#;

/// Tool-call instructions for the no-speech case. Always appended to whatever
/// system prompt the user has configured — not user-editable, because the
/// model needs explicit guidance on when to invoke `report_no_speech`.
const NO_SPEECH_TOOL_PROMPT: &str = r#"空音频处理（重要，不可忽略）：
- 如果音频中没有可识别的语音内容（例如：完全静音、只有环境噪音、只有键盘/鼠标声、模型听不清任何字词、用户没有真正说话），必须调用 `report_no_speech` 工具，并且不要输出任何文本（不要解释，不要"无内容"、"未检测到语音"之类的提示，不要标点，也不要空格）。
- 只要识别到了任何有意义的语句，就正常按上述规则润色输出文本，并且不要调用 `report_no_speech` 工具。"#;

#[derive(Deserialize)]
struct ResponsesReply {
    #[serde(default)]
    output: Vec<OutputItem>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct OutputItem {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    content: Vec<ContentPart>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct ContentPart {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

/// Encode 16kHz mono f32 samples into a 16-bit PCM WAV byte buffer.
fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buf = Vec::with_capacity(samples.len() * 2 + 44);
    {
        let cursor = Cursor::new(&mut buf);
        let mut writer = hound::WavWriter::new(cursor, spec)
            .map_err(|e| format!("WAV writer init failed: {}", e))?;

        for &s in samples {
            let clamped = s.clamp(-1.0, 1.0);
            let pcm = (clamped * 32767.0) as i16;
            writer
                .write_sample(pcm)
                .map_err(|e| format!("WAV write failed: {}", e))?;
        }

        writer
            .finalize()
            .map_err(|e| format!("WAV finalize failed: {}", e))?;
    }

    Ok(buf)
}

pub struct DoubaoEngine {
    client: reqwest::blocking::Client,
}

impl DoubaoEngine {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build HTTP client");
        Self { client }
    }

    /// Transcribe 16kHz mono f32 samples via Doubao Responses API.
    ///
    /// `system_prompt` controls the model's transcription/polishing behavior;
    /// pass an empty string to fall back to [`DEFAULT_SYSTEM_PROMPT`].
    pub fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        api_key: &str,
        model: &str,
        prompt: &str,
        system_prompt: &str,
    ) -> Result<String, String> {
        if api_key.trim().is_empty() {
            return Err("API key is empty — set ARK API key in Settings".to_string());
        }

        let wav_bytes = encode_wav(samples, sample_rate)?;
        if wav_bytes.len() > 25 * 1024 * 1024 {
            return Err(format!(
                "Audio too large for base64 mode: {} KB (limit 25 MB)",
                wav_bytes.len() / 1024
            ));
        }

        let b64 = B64.encode(&wav_bytes);
        let audio_url = format!("data:audio/wav;base64,{}", b64);

        let base_prompt = if system_prompt.trim().is_empty() {
            DEFAULT_SYSTEM_PROMPT
        } else {
            system_prompt
        };
        let instructions = format!("{}\n\n{}", base_prompt, NO_SPEECH_TOOL_PROMPT);

        let body = serde_json::json!({
            "model": model,
            "stream": false,
            "thinking": { "type": "disabled" },
            "instructions": instructions,
            "tools": [
                {
                    "type": "function",
                    "name": "report_no_speech",
                    "description": "当音频中没有可识别的语音内容（完全静音、只有环境噪音、键盘/鼠标声、听不清任何字词、用户没有真正说话）时调用。调用此工具后不要再输出任何文本。",
                    "parameters": {
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                    }
                }
            ],
            "input": [
                {
                    "role": "user",
                    "content": [
                        { "type": "input_audio", "audio_url": audio_url },
                        { "type": "input_text", "text": prompt },
                    ],
                }
            ],
        });

        let res = self
            .client
            .post(ENDPOINT)
            .header("Content-Type", "application/json")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = res.status();
        let text = res
            .text()
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if !status.is_success() {
            return Err(format!(
                "Doubao API HTTP {}: {}",
                status,
                truncate(&text, 500)
            ));
        }

        let parsed: ResponsesReply = serde_json::from_str(&text).map_err(|e| {
            format!(
                "Invalid response JSON: {} | body: {}",
                e,
                truncate(&text, 300)
            )
        })?;

        if let Some(err) = parsed.error {
            return Err(format!(
                "Doubao API error [{}]: {}",
                err.code.unwrap_or_default(),
                err.message.unwrap_or_default()
            ));
        }

        for item in &parsed.output {
            if item.kind == "function_call" && item.name.as_deref() == Some("report_no_speech") {
                return Ok(String::new());
            }
        }

        let mut out = String::new();
        for item in parsed.output {
            for part in item.content {
                if part.kind == "output_text" {
                    if let Some(t) = part.text {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(&t);
                    }
                }
            }
        }

        Ok(out.trim().to_string())
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let trimmed: String = s.chars().take(max).collect();
        format!("{}…", trimmed)
    }
}
