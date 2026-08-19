use crate::credential_store;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use url::Url;
use zeroize::Zeroize;

const PROFILE_SCHEMA_VERSION: &str = "0.1";
const PROFILE_FILE_NAME: &str = "model-provider.json";
const CREDENTIAL_ACCOUNT: &str = "jingwei.openai";
const MAX_PROFILE_BYTES: u64 = 32 * 1024;
const MAX_API_KEY_BYTES: usize = 2560;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelProviderProfile {
    schema_version: String,
    provider: String,
    base_url: String,
    model: String,
    credential_ref: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveModelConfigurationInput {
    provider: String,
    base_url: String,
    model: String,
    api_key: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EnvironmentModelConfiguration {
    api_key_present: bool,
    model_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    configured: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelConfigurationView {
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<ModelProviderProfile>,
    credential_configured: bool,
    credential_store_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    credential_store_reason: Option<String>,
    environment: EnvironmentModelConfiguration,
}

#[tauri::command]
pub(crate) fn model_configuration_get(app: AppHandle) -> Result<ModelConfigurationView, String> {
    read_configuration_view(&app)
}

#[tauri::command]
pub(crate) fn model_configuration_save(
    app: AppHandle,
    mut input: SaveModelConfigurationInput,
) -> Result<ModelConfigurationView, String> {
    let profile = normalize_profile(&input)?;
    let mut secret = input.api_key.take();
    if let Some(value) = secret.as_mut() {
        if let Err(error) = validate_api_key(value) {
            value.zeroize();
            return Err(error);
        }
    }
    if let Err(error) = write_profile(&app, &profile) {
        if let Some(value) = secret.as_mut() {
            value.zeroize();
        }
        return Err(error);
    }
    if let Some(mut secret) = secret {
        let write_result = credential_store::write(&secret);
        secret.zeroize();
        write_result?;
    }
    read_configuration_view(&app)
}

#[tauri::command]
pub(crate) fn model_configuration_delete_credential(
    app: AppHandle,
) -> Result<ModelConfigurationView, String> {
    credential_store::delete()?;
    read_configuration_view(&app)
}

#[tauri::command]
pub(crate) fn validate_credential_reference(reference: String) -> Result<String, String> {
    credential_account(&reference)?;
    Ok(reference)
}

fn read_configuration_view(app: &AppHandle) -> Result<ModelConfigurationView, String> {
    let profile = read_profile(app)?;
    let environment = environment_configuration();
    let (credential_store_available, credential_configured, credential_store_reason) =
        match credential_store::read() {
            Ok(Some(mut secret)) => {
                let configured = !secret.is_empty();
                secret.zeroize();
                (true, configured, None)
            }
            Ok(None) => (true, false, None),
            Err(reason) => (false, false, Some(reason)),
        };

    Ok(ModelConfigurationView {
        profile,
        credential_configured,
        credential_store_available,
        credential_store_reason,
        environment,
    })
}

fn normalize_profile(input: &SaveModelConfigurationInput) -> Result<ModelProviderProfile, String> {
    if input.provider != "openai-responses" {
        return Err("当前版本只支持 OpenAI Responses provider".into());
    }

    let model = input.model.trim();
    if model.is_empty()
        || model.len() > 128
        || !model.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | '/' | ':')
        })
    {
        return Err("模型 ID 必须为 1–128 位字母、数字、点、横线、下划线、斜线或冒号".into());
    }

    let parsed = Url::parse(input.base_url.trim()).map_err(|_| "API Base URL 无效".to_string())?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err("API Base URL 必须使用 HTTPS".into());
    }
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("API Base URL 不能包含凭据、查询参数或片段".into());
    }

    Ok(ModelProviderProfile {
        schema_version: PROFILE_SCHEMA_VERSION.into(),
        provider: input.provider.clone(),
        base_url: parsed.to_string().trim_end_matches('/').to_string(),
        model: model.to_string(),
        credential_ref: credential_reference(),
    })
}

fn validate_api_key(secret: &str) -> Result<(), String> {
    if secret != secret.trim()
        || !(8..=MAX_API_KEY_BYTES).contains(&secret.len())
        || !secret.is_ascii()
        || secret.chars().any(char::is_control)
    {
        return Err("API Key 格式无效；禁止首尾空白、控制字符或超长内容".into());
    }
    Ok(())
}

fn environment_configuration() -> EnvironmentModelConfiguration {
    let api_key_present = env::var("OPENAI_API_KEY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let model = env::var("JINGWEI_OPENAI_MODEL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value.len() <= 128);
    EnvironmentModelConfiguration {
        api_key_present,
        model_present: model.is_some(),
        configured: api_key_present && model.is_some(),
        model,
    }
}

fn read_profile(app: &AppHandle) -> Result<Option<ModelProviderProfile>, String> {
    let path = profile_path(app)?;
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("无法读取本机模型配置元数据".into()),
    };
    if metadata.len() > MAX_PROFILE_BYTES {
        return Err("本机模型配置文件异常超限".into());
    }
    let bytes = fs::read(path).map_err(|_| "无法读取本机模型配置".to_string())?;
    let profile: ModelProviderProfile =
        serde_json::from_slice(&bytes).map_err(|_| "本机模型配置格式无效".to_string())?;
    if profile.schema_version != PROFILE_SCHEMA_VERSION {
        return Err("本机模型配置版本不兼容".into());
    }
    Ok(Some(profile))
}

fn write_profile(app: &AppHandle, profile: &ModelProviderProfile) -> Result<(), String> {
    let path = profile_path(app)?;
    let parent = path
        .parent()
        .ok_or_else(|| "本机模型配置目录无效".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "无法创建本机模型配置目录".to_string())?;
    let bytes = serde_json::to_vec_pretty(profile).map_err(|_| "无法序列化模型配置".to_string())?;
    fs::write(path, bytes).map_err(|_| "无法保存本机模型配置".to_string())
}

fn profile_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join(PROFILE_FILE_NAME))
        .map_err(|_| "无法定位本机应用配置目录".to_string())
}

fn credential_reference() -> String {
    format!("credential://{}/{}", platform_name(), CREDENTIAL_ACCOUNT)
}

fn credential_account(reference: &str) -> Result<&str, String> {
    let mut segments = reference.split('/');
    let scheme = segments.next();
    let empty = segments.next();
    let platform = segments.next();
    let account = segments.next();
    if scheme != Some("credential:")
        || empty != Some("")
        || !matches!(platform, Some("windows" | "macos" | "linux"))
        || segments.next().is_some()
    {
        return Err("凭据引用必须使用 credential://<platform>/<name> 格式".into());
    }
    let account = account.ok_or_else(|| "凭据引用缺少名称".to_string())?;
    let valid_length = (3..=80).contains(&account.len());
    let valid_characters = account.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '_' | '-')
    });
    if !valid_length || !valid_characters {
        return Err("凭据引用名称格式无效".into());
    }
    Ok(account)
}

fn platform_name() -> &'static str {
    match env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        _ => "linux",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_input() -> SaveModelConfigurationInput {
        SaveModelConfigurationInput {
            provider: "openai-responses".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-example-1".into(),
            api_key: None,
        }
    }

    #[test]
    fn normalizes_non_secret_profile() {
        let profile = normalize_profile(&valid_input()).unwrap();
        assert_eq!(profile.provider, "openai-responses");
        assert_eq!(profile.base_url, "https://api.openai.com/v1");
        assert_eq!(profile.model, "gpt-example-1");
        assert!(profile.credential_ref.starts_with("credential://"));
        let serialized = serde_json::to_string(&profile).unwrap();
        assert!(!serialized.contains("apiKey"));
    }

    #[test]
    fn rejects_unsafe_endpoint_and_model() {
        let mut input = valid_input();
        input.base_url = "http://api.openai.com/v1".into();
        assert!(normalize_profile(&input).is_err());
        input.base_url = "https://user:secret@example.com/v1".into();
        assert!(normalize_profile(&input).is_err());
        input.base_url = "https://api.openai.com/v1".into();
        input.model = "bad model".into();
        assert!(normalize_profile(&input).is_err());
    }

    #[test]
    fn rejects_secret_whitespace_and_control_characters() {
        assert!(validate_api_key(" short ").is_err());
        assert!(validate_api_key("valid-key\nvalue").is_err());
        assert!(validate_api_key("valid-test-key").is_ok());
    }

    #[test]
    fn validates_opaque_cross_platform_references() {
        assert_eq!(
            credential_account("credential://windows/jingwei.openai").unwrap(),
            "jingwei.openai"
        );
        assert!(credential_account("credential://windows/UPPER").is_err());
        assert!(credential_account("secret-value").is_err());
    }
}
