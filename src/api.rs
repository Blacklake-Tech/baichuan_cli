use chrono::{self};
use clap::ValueEnum;
use log::debug;
use md5::compute;
use reqwest::{self, StatusCode};
use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;
use std::collections::HashMap;
use ulid::Ulid;

fn md5_hash(s: &str) -> String {
    format!("{:x}", compute(s))
}

#[derive(Serialize, Debug)]
struct BaichuanReq {
    model: Model,
    messages: Vec<ChatMessage>,
    parameters: Parameters,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

// 成功	成功	0	success	请求成功并获得预期的结果
// 请求错误	失败	1	system error	请求失败
// 请求错误	参数非法	10000	Invalid parameters, please check	请求中的参数不合法，请仔细检查
// 请求错误	私钥缺失	10100	Missing apikey	请求缺少必要的私钥参数
// 请求错误	私钥非法	10101	Invalid apikey	提供的私钥无效，无法解码
// 请求错误	私钥过期	10102	apikey has expired	提供的私钥已过期，如果非永久有效，需重新获取
// 请求错误	时间戳无效	10103	Invalid Timestamp parameter in request header	错误的时间戳格式
// 请求错误	时间戳过期	10104	Expire Timestamp parameter in request header	过期的时间戳
// 请求错误	无效签名	10105	Invalid Signature parameter in request header	请求头中的签名无效
// 请求错误	无效加密算法	10106	Invalid encryption algorithm in request header, not supported by server	请求头中的加密算法不被支持
// 账号错误	账号未知	10200	Account not found	请求的账号不存在
// 账号错误	账号锁定	10201	Account is locked, please contact the support staff	请求的账号已锁定，请联系支持人员
// 账号错误	账号临时锁定	10202	Account is temporarily locked, please try again later	请求的账号临时锁定，请稍后再试
// 账号错误	账号请求频繁	10203	Request too frequent, please try again later	请求过于频繁，已触发频率控制。当前单 apikey 限制 10rpm
// 账号错误	账号余额不足	10300	Insufficient account balance, please recharge	账号余额不足，请进行充值
// 账号错误	账户未认证	10301	Account is not verified, please complete the verification first	账号未认证，请先认证通过
// 安全错误	prompt 不安全	10400	Topic violates security policy	返回的 prompt 内容不符合安全策略
// 安全错误	answer 不安全	10401	Topic violates security policy	返回的 answer 内容不符合安全策略
// 服务错误	服务内部错误	10500	Internal error	服务内部发生错误，请稍后再试
#[derive(Deserialize_repr, PartialEq, Debug)]
#[repr(i32)]
pub enum RespCode {
    Success = 0,
    SystemError = 1,
    InvalidParameters = 10000,
    MissingApikey = 10100,
    InvalidApikey = 10101,
    ApikeyExpired = 10102,
    InvalidTimestamp = 10103,
    ExpireTimestamp = 10104,
    InvalidSignature = 10105,
    InvalidEncryptionAlgorithm = 10106,
    AccountNotFound = 10200,
    AccountLocked = 10201,
    AccountTempLocked = 10202,
    AccountRequestTooFrequent = 10203,
    AccountBalanceInsufficient = 10300,
    AccountNotVerified = 10301,
    PromptNotSafe = 10400,
    AnswerNotSafe = 10401,
    InternalError = 10500,
}

#[derive(Deserialize, Debug)]
pub struct UsageInfo {
    pub prompt_tokens: i64,
    pub answer_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Deserialize, Debug)]
pub struct BaichuanData {
    pub messages: Vec<ChatMessage>,
}

#[derive(Deserialize, Debug)]
pub struct BaichuanResp {
    pub code: RespCode,
    pub msg: String,
    #[serde(default)]
    pub data: Option<BaichuanData>,
    #[serde(default)]
    pub usage: Option<UsageInfo>,
}

#[derive(Serialize, Debug)]
struct Parameters(HashMap<String, String>);

fn generate_header(
    api_key: &String,
    secret_key: &String,
    data: &BaichuanReq,
) -> Result<(HashMap<String, String>, String), String> {
    // current timestamp in seconds
    let timestamp = chrono::Utc::now().timestamp();
    let serialized_request = serde_json::to_string(&data).map_err(|e| e.to_string())?;
    let signature = md5_hash(&format!(
        "{}{}{}",
        secret_key, serialized_request, timestamp,
    ));
    let request_id = Ulid::new();
    let headers = HashMap::from([
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Authorization".to_string(), format!("Bearer {}", &api_key)),
        ("X-BC-Request-Id".to_string(), request_id.to_string()),
        ("X-BC-Timestamp".to_string(), timestamp.to_string()),
        ("X-BC-Signature".to_string(), signature),
        ("X-BC-Sign-Algo".to_string(), "MD5".to_string()),
    ]);
    Ok((headers, request_id.to_string()))
}

#[derive(Serialize, PartialEq, ValueEnum, Clone, Copy, Debug)]
pub enum Model {
    #[serde(rename = "Baichuan2-53B")]
    Baichuan2_53B,
}

const URL: &str = "https://api.baichuan-ai.com/v1/chat";

pub async fn make_baichuan_request(
    api_key: &String,
    secret_key: &String,
    model: Model,
    messages: Vec<String>,
) -> Result<BaichuanResp, String> {
    let request = BaichuanReq {
        model,
        messages: messages
            .into_iter()
            .map(|m| ChatMessage {
                role: "user".into(),
                content: m,
                finish_reason: None,
            })
            .collect(),
        parameters: Parameters(HashMap::default()),
    };
    let (headers, req_id) = generate_header(api_key, secret_key, &request)?;
    let client = reqwest::Client::new();
    let headers = (&headers).try_into().expect("failed to convert to error");
    debug!("starting request {}", req_id);
    let response = client
        .post(URL)
        .headers(headers)
        .json(&request)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status() == StatusCode::OK {
        debug!("request {} was successful", req_id);
        match response.json().await {
            Ok(resp) => Ok(resp),
            Err(e) => Err(format!("failed to parse json: {}", e)),
        }
    } else {
        Err(format!(
            "failed to send request: {:?}",
            response.text().await
        ))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_md5_hash() {
        assert_eq!("5eb63bbbe01eeed093cb22bb8f5acdc3", md5_hash("hello world"));
    }

    #[test]
    fn test_gen_header() -> Result<(), String> {
        let api_key = &"api key".to_string();
        let secret_key = &"secret key".to_string();
        let data = BaichuanReq {
            model: Model::Baichuan2_53B,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "1".to_string(),
                finish_reason: None,
            }],
            parameters: Parameters(HashMap::default()),
        };
        let (generated_header, _) = generate_header(api_key, secret_key, &data)?;
        let content_type = generated_header.get("Content-Type");
        assert_eq!(Some(&"application/json".to_string()), content_type);
        Ok(())
    }

    #[test]
    fn test_resp_deser() {
        let text = r#"{"code":0,"msg":"success","data":{"messages":[{"role":"assistant","content":"你好！很高兴为您提供帮助。请问您有什么问题需要我解答？","finish_reason":"stop"}]},"usage":{"prompt_tokens":3,"answer_tokens":15,"total_tokens":18}}"#;
        let parsed: BaichuanResp = serde_json::from_str(text).expect("cannot parse");
        assert!(parsed.data.is_some());
        assert_eq!(parsed.data.unwrap().messages.len(), 1);
    }
}
