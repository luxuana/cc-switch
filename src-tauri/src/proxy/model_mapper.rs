//! 模型映射模块
//!
//! 在请求转发前，根据 Provider 配置替换请求中的模型名称

use crate::claude_desktop_config::ONE_M_CONTEXT_MARKER;
use crate::provider::{ModelFamilyRoute, ModelRole, Provider};
use serde_json::Value;

/// 模型映射配置
pub struct ModelMapping {
    pub haiku_model: Option<String>,
    pub sonnet_model: Option<String>,
    pub opus_model: Option<String>,
    pub fable_model: Option<String>,
    pub subagent_model: Option<String>,
    pub default_model: Option<String>,
    /// subagent 路由的目标模型（来自 `meta.modelFamilyRoutes["subagent"].model`）。
    ///
    /// `CLAUDE_CODE_SUBAGENT_MODEL` 是 subagent **检测键**（识别 subagent 请求），
    /// 不能把路由 model 写进它（否则检测失效）。命中 subagent 检测时优先返回本目标。
    pub subagent_target: Option<String>,
    /// 所有角色路由的目标模型集合（`meta.modelFamilyRoutes[*].model`，含 subagent）。
    ///
    /// 请求模型精确命中任一目标（忽略 [1M]）→ 直接透传：接管时 CC 端
    /// `ANTHROPIC_DEFAULT_*_MODEL` 已写入路由目标，请求模型即目标本身；透传避免
    /// 落到默认兜底改写（厂商别名等不含家族子串的目标模型不会被家族匹配命中）。
    pub route_targets: Vec<String>,
}

impl ModelMapping {
    /// 从 Provider 配置中提取模型映射
    pub fn from_provider(provider: &Provider) -> Self {
        let env = provider.settings_config.get("env");
        let subagent_target = provider
            .meta
            .as_ref()
            .and_then(|meta| meta.model_family_routes.get(ModelRole::Subagent.as_str()))
            .and_then(|route| route.model.as_deref())
            .filter(|s| !s.is_empty())
            .map(String::from);
        let route_targets = provider
            .meta
            .as_ref()
            .map(|meta| {
                meta.model_family_routes
                    .values()
                    .filter_map(|route| route.model.as_deref())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        Self {
            haiku_model: env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_HAIKU_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            sonnet_model: env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_SONNET_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            opus_model: env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_OPUS_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            fable_model: env
                .and_then(|e| e.get("ANTHROPIC_DEFAULT_FABLE_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            subagent_model: env
                .and_then(|e| e.get("CLAUDE_CODE_SUBAGENT_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            default_model: env
                .and_then(|e| e.get("ANTHROPIC_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from),
            subagent_target,
            route_targets,
        }
    }

    /// 检查是否配置了任何模型映射
    pub fn has_mapping(&self) -> bool {
        self.haiku_model.is_some()
            || self.sonnet_model.is_some()
            || self.opus_model.is_some()
            || self.fable_model.is_some()
            || self.subagent_model.is_some()
            || self.default_model.is_some()
            || self.subagent_target.is_some()
            || !self.route_targets.is_empty()
    }

    /// 根据原始模型名称获取映射后的模型
    pub fn map_model(&self, original_model: &str) -> String {
        let model_lower = original_model.to_lowercase();

        // 0. 请求模型 == 任一路由目标模型（忽略 [1M]）→ 直接透传。
        //    接管时 CC 端 `_MODEL` 已写入路由目标，请求模型即目标本身；
        //    透传避免落到默认兜底改写（厂商别名等不含家族子串的目标模型
        //    不会被下面的家族匹配命中，§9.3 [HIGH] 同类问题推广到所有角色）。
        for target in &self.route_targets {
            if strip_one_m_suffix_for_upstream(original_model)
                == strip_one_m_suffix_for_upstream(target)
            {
                return original_model.to_string();
            }
        }

        // 1. 按模型类型匹配（家族角色优先，与 resolve_model_family_route 的
        //    路由优先级一致；§9.2 [LOW, accepted]：subagent 模型名含家族子串
        //    会被家族映射抢先命中）。
        if model_lower.contains("fable") {
            if let Some(ref m) = self.fable_model {
                return m.clone();
            }
            // 未单独配置 fable 档时归入 opus 档，与 Claude Code 官方
            // 分类器降级方向一致（fable→opus），避免落到 default 失去层级。
            if let Some(ref m) = self.opus_model {
                return m.clone();
            }
        }
        if model_lower.contains("haiku") {
            if let Some(ref m) = self.haiku_model {
                return m.clone();
            }
        }
        if model_lower.contains("opus") {
            if let Some(ref m) = self.opus_model {
                return m.clone();
            }
        }
        if model_lower.contains("sonnet") {
            if let Some(ref m) = self.sonnet_model {
                return m.clone();
            }
        }

        // 2. subagent 处理。
        //    CLAUDE_CODE_SUBAGENT_MODEL（subagent_model）是「检测键」：请求模型
        //    命中检测键即识别为 subagent 请求。有 subagent 路由目标（subagent_target，
        //    来自 meta.model_family_routes["subagent"].model）时映射到目标模型；
        //    无路由目标则保留原模型（向后兼容，原行为不变）。
        if let Some(ref m) = self.subagent_model {
            if strip_one_m_suffix_for_upstream(original_model) == strip_one_m_suffix_for_upstream(m)
            {
                if let Some(ref target) = self.subagent_target {
                    return target.clone();
                }
                return original_model.to_string();
            }
        }
        // （请求模型 == subagent 路由目标 → 透传的分支已由上面的通用
        //   route_targets 覆盖，subagent 目标也在 route_targets 集合中。）

        // 3. 默认模型
        if let Some(ref m) = self.default_model {
            return m.clone();
        }

        // 4. 无映射，保持原样
        original_model.to_string()
    }
}

/// 对请求体应用模型映射
///
/// 返回 (映射后的请求体, 原始模型名, 映射后模型名)
pub fn apply_model_mapping(
    mut body: Value,
    provider: &Provider,
) -> (Value, Option<String>, Option<String>) {
    let mapping = ModelMapping::from_provider(provider);

    // 如果没有配置映射，直接返回
    if !mapping.has_mapping() {
        let original = body.get("model").and_then(|m| m.as_str()).map(String::from);
        return (body, original, None);
    }

    // 提取原始模型名
    let original_model = body.get("model").and_then(|m| m.as_str()).map(String::from);

    if let Some(ref original) = original_model {
        let mapped = mapping.map_model(original);

        if mapped != *original {
            log::debug!("[ModelMapper] 模型映射: {original} → {mapped}");
            body["model"] = serde_json::json!(mapped);
            return (body, Some(original.clone()), Some(mapped));
        }
    }

    (body, original_model, None)
}

/// Claude Code 通过 `[1M]` 后缀声明 100 万上下文能力；上游 API
/// 通常不接受这个本地能力标记，转发前需要剥离。
pub fn strip_one_m_suffix_for_upstream(model: &str) -> &str {
    let trimmed = model.trim_end();
    let marker = ONE_M_CONTEXT_MARKER.as_bytes();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= marker.len()
        && bytes[bytes.len() - marker.len()..].eq_ignore_ascii_case(marker)
    {
        return trimmed[..trimmed.len() - marker.len()].trim_end();
    }
    model
}

pub fn strip_one_m_suffix_for_upstream_from_body(mut body: Value) -> Value {
    let Some(model) = body.get("model").and_then(Value::as_str) else {
        return body;
    };

    let stripped = strip_one_m_suffix_for_upstream(model);
    if stripped != model {
        log::debug!("[ModelMapper] 去除本地 1M 标记: {model} → {stripped}");
        body["model"] = serde_json::json!(stripped);
    }
    body
}

// ============================================================================
// 模型家族路由（Model Family Routing）
// ============================================================================

/// 根据请求模型名判定模型角色（家族子串匹配）。
///
/// 复用 `map_model` 的角色匹配逻辑（同一子串集合、同一优先级：fable → haiku →
/// opus → sonnet）。subagent 无法仅凭模型名判定（subagent 检测键是 Provider 级
/// 配置），由 [`resolve_model_family_route`] 依据 Provider 的检测键/路由目标做
/// 精确匹配。
pub fn classify_model_role(model: &str) -> Option<ModelRole> {
    let model_lower = model.to_lowercase();
    if model_lower.contains("fable") {
        return Some(ModelRole::Fable);
    }
    if model_lower.contains("haiku") {
        return Some(ModelRole::Haiku);
    }
    if model_lower.contains("opus") {
        return Some(ModelRole::Opus);
    }
    if model_lower.contains("sonnet") {
        return Some(ModelRole::Sonnet);
    }
    None
}

/// 从请求 body 解析命中的模型家族路由。
///
/// 返回 `(角色, 路由配置)`；仅当路由 `base_url` 非空时视为有效（§8.3 判空）。
///
/// 匹配优先级：
/// 1. **路由「目标模型」精确匹配**：请求 body.model 与某角色路由的 `model`
///    （弹窗「目标模型」输入框）精确匹配（忽略 [1M] 后缀与大小写）即命中。
///    接管时 CC 端 `_MODEL` 已写入路由目标，请求模型即目标本身——稳定命中；
///    厂商别名（不含家族子串）也能命中。对所有角色生效（含 subagent 目标）。
/// 2. 家族子串匹配（`classify_model_role`）：无精确命中时按家族子串判定角色，
///    与 `map_model` 的角色匹配优先级一致。
/// 3. subagent 检测键匹配（精确匹配，忽略 [1M]）：请求模型 == Provider 的
///    `CLAUDE_CODE_SUBAGENT_MODEL` 检测键（subagent 路由目标的精确匹配已由
///    步骤 1 覆盖）。
pub fn resolve_model_family_route(
    provider: &Provider,
    body: &Value,
) -> Option<(ModelRole, ModelFamilyRoute)> {
    let model = body.get("model").and_then(Value::as_str)?;
    let meta = provider.meta.as_ref()?;
    let model_stripped = strip_one_m_suffix_for_upstream(model);

    // 1. 路由「目标模型」精确匹配优先（对所有角色；subagent 也走同一逻辑，
    //    覆盖 subagent 路由目标与请求模型一致的情况）
    for (role_str, route) in &meta.model_family_routes {
        if route.base_url.trim().is_empty() {
            continue;
        }
        let matched = route
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|target| {
                strip_one_m_suffix_for_upstream(target)
                    .eq_ignore_ascii_case(model_stripped)
            })
            .unwrap_or(false);
        if matched {
            // role_str 与 ModelRole::as_str() 对齐（"sonnet"/"opus"/"fable"/"haiku"/"subagent"），
            // 未知 key 直接忽略（不影响其它角色）。
            let role = match role_str.as_str() {
                "sonnet" => ModelRole::Sonnet,
                "opus" => ModelRole::Opus,
                "fable" => ModelRole::Fable,
                "haiku" => ModelRole::Haiku,
                "subagent" => ModelRole::Subagent,
                _ => continue,
            };
            return Some((role, route.clone()));
        }
    }

    // 2. 家族子串匹配优先
    if let Some(role) = classify_model_role(model) {
        if let Some(route) = meta.model_family_routes.get(role.as_str()) {
            if !route.base_url.trim().is_empty() {
                return Some((role, route.clone()));
            }
        }
    }

    // 3. subagent 检测键匹配（精确匹配，忽略 [1M]；路由目标匹配已由步骤 1 覆盖）
    if let Some(route) = meta.model_family_routes.get(ModelRole::Subagent.as_str()) {
        if !route.base_url.trim().is_empty() {
            let matches_detection = provider
                .settings_config
                .get("env")
                .and_then(|e| e.get("CLAUDE_CODE_SUBAGENT_MODEL"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|detect| {
                    strip_one_m_suffix_for_upstream(model)
                        == strip_one_m_suffix_for_upstream(detect)
                })
                .unwrap_or(false);
            if matches_detection {
                return Some((ModelRole::Subagent, route.clone()));
            }
        }
    }

    None
}

/// 命中路由时派生 Provider：改写 env base_url/auth + meta.api_format + 角色模型 env 键。
///
/// Immutable：返回新 Provider，不修改原对象。下游协议转换 / 认证 / URL 构建
/// 均读取 Provider 上的独立配置，因此零改动。
pub fn derive_routed_provider(
    provider: &Provider,
    role: ModelRole,
    route: &ModelFamilyRoute,
) -> Provider {
    let mut routed = provider.clone();

    // settings_config 约定为对象；兜底为空对象避免后续索引写入 panic。
    if !routed.settings_config.is_object() {
        routed.settings_config = serde_json::json!({});
    }

    // 1. env：base_url + auth
    let mut env = routed
        .settings_config
        .get("env")
        .and_then(|e| e.as_object())
        .cloned()
        .unwrap_or_default();
    env.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        serde_json::json!(route.base_url.trim_end_matches('/')),
    );
    let key_field = route
        .api_key_field
        .as_deref()
        .unwrap_or("ANTHROPIC_AUTH_TOKEN");
    if let Some(key) = route.api_key.as_deref() {
        env.insert(key_field.to_string(), serde_json::json!(key));
    }

    // 2. 角色模型 env 键
    let role_env_key = role.env_key();
    match route.model.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(model) => {
            // §9.2a [HIGH]：subagent 角色的检测键 CLAUDE_CODE_SUBAGENT_MODEL 不可改写
            //（改写会导致 subagent 检测失效）。subagent 路由目标由
            // ModelMapping.subagent_target（读 meta.model_family_routes["subagent"].model）承担。
            if role != ModelRole::Subagent {
                env.insert(role_env_key.to_string(), serde_json::json!(model));
            }
        }
        None => {
            // §9.2b [MEDIUM]：route.model 为空 → 删除该角色 env 键（透传原模型）。
            if role != ModelRole::Subagent {
                env.remove(role_env_key);
                // 非 subagent 角色删除 ANTHROPIC_MODEL：防默认兜底改写请求模型
                //（UniversalProvider::to_claude_provider 必写 ANTHROPIC_MODEL）。
                env.remove("ANTHROPIC_MODEL");
            }
            // subagent：保留检测键——map_model 的 subagent 分支命中时本就返回原模型
            //（透传）；删除会让检测失效、请求落到默认兜底，违背「透传」意图。
        }
    }

    routed.settings_config["env"] = serde_json::Value::Object(env);

    // 3. §9.2c [MEDIUM]：清除继承自顶层 settings_config 的 URL 提示字段
    //   （base_url / baseURL / baseUrl / apiEndpoint）。真实 URL 已写入
    //   env.ANTHROPIC_BASE_URL，清除这些旧字段防止 reasoning-effort 等路径
    //   读到主 Provider 的旧 URL 误判上游能力（xhigh 过度 clamp）。
    if let Some(obj) = routed.settings_config.as_object_mut() {
        for stale in ["base_url", "baseURL", "baseUrl", "apiEndpoint"] {
            obj.remove(stale);
        }
    }

    // 4. 协议格式：写入 meta.api_format（SSOT，get_claude_api_format 优先读取）。
    //    路由未指定格式时保留继承自主 Provider 的 meta.api_format。
    //    同时把 route.is_full_url 写入 meta.is_full_url——forward 的 is_full_url
    //    判定（forwarder.rs）读 meta.is_full_url，若此处不传递，前端已保存的
    //    "完整端点模式"开关将不生效。
    if route.api_format.is_some() || route.is_full_url.is_some() {
        let mut meta = routed.meta.clone().unwrap_or_default();
        if let Some(api_format) = route.api_format.as_deref() {
            meta.api_format = Some(api_format.to_string());
        }
        if let Some(is_full_url) = route.is_full_url {
            meta.is_full_url = Some(is_full_url);
        }
        routed.meta = Some(meta);
    }

    routed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_provider_with_mapping() -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test".to_string(),
            settings_config: json!({
                "env": {
                    "ANTHROPIC_MODEL": "default-model",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "haiku-mapped",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-mapped",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-mapped",
                    "ANTHROPIC_DEFAULT_FABLE_MODEL": "fable-mapped"
                }
            }),
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    fn create_provider_without_mapping() -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test".to_string(),
            settings_config: json!({}),
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn test_sonnet_mapping() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "claude-sonnet-4-5-20250929"});
        let (result, original, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "sonnet-mapped");
        assert_eq!(original, Some("claude-sonnet-4-5-20250929".to_string()));
        assert_eq!(mapped, Some("sonnet-mapped".to_string()));
    }

    #[test]
    fn test_haiku_mapping() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "claude-haiku-4-5"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "haiku-mapped");
        assert_eq!(mapped, Some("haiku-mapped".to_string()));
    }

    #[test]
    fn test_opus_mapping() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "claude-opus-4-5"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "opus-mapped");
        assert_eq!(mapped, Some("opus-mapped".to_string()));
    }

    #[test]
    fn test_fable_mapping() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "claude-fable-5"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "fable-mapped");
        assert_eq!(mapped, Some("fable-mapped".to_string()));
    }

    #[test]
    fn test_fable_with_one_m_suffix_mapping() {
        // Claude Code 实际会发 claude-fable-5[1m] 形态（issue #3980）
        let provider = create_provider_with_mapping();
        let body = json!({"model": "claude-fable-5[1m]"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "fable-mapped");
        assert_eq!(mapped, Some("fable-mapped".to_string()));
    }

    #[test]
    fn test_fable_falls_back_to_opus_when_unset() {
        let mut provider = create_provider_with_mapping();
        provider.settings_config = json!({
            "env": {
                "ANTHROPIC_MODEL": "default-model",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-mapped"
            }
        });
        let body = json!({"model": "claude-fable-5"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "opus-mapped");
        assert_eq!(mapped, Some("opus-mapped".to_string()));
    }

    #[test]
    fn test_fable_falls_back_to_default_without_opus() {
        let mut provider = create_provider_with_mapping();
        provider.settings_config = json!({
            "env": {
                "ANTHROPIC_MODEL": "default-model"
            }
        });
        let body = json!({"model": "claude-fable-5"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "default-model");
        assert_eq!(mapped, Some("default-model".to_string()));
    }

    #[test]
    fn test_thinking_does_not_affect_model_mapping() {
        // Issue #2081: thinking 参数不应影响模型映射
        let provider = create_provider_with_mapping();
        let body = json!({
            "model": "claude-sonnet-4-5",
            "thinking": {"type": "enabled"}
        });
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "sonnet-mapped");
        assert_eq!(mapped, Some("sonnet-mapped".to_string()));
    }

    #[test]
    fn test_thinking_adaptive_does_not_affect_model_mapping() {
        // Issue #2081: adaptive thinking 也不应影响模型映射
        let provider = create_provider_with_mapping();
        let body = json!({
            "model": "claude-sonnet-4-5",
            "thinking": {"type": "adaptive"}
        });
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "sonnet-mapped");
        assert_eq!(mapped, Some("sonnet-mapped".to_string()));
    }

    #[test]
    fn test_thinking_disabled() {
        let provider = create_provider_with_mapping();
        let body = json!({
            "model": "claude-sonnet-4-5",
            "thinking": {"type": "disabled"}
        });
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "sonnet-mapped");
        assert_eq!(mapped, Some("sonnet-mapped".to_string()));
    }

    #[test]
    fn test_unknown_model_uses_default() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "some-unknown-model"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "default-model");
        assert_eq!(mapped, Some("default-model".to_string()));
    }

    #[test]
    fn test_subagent_model_preserved_before_default_fallback() {
        let mut provider = create_provider_with_mapping();
        provider.settings_config = json!({
            "env": {
                "ANTHROPIC_MODEL": "default-model",
                "CLAUDE_CODE_SUBAGENT_MODEL": "gpt-5.4-mini"
            }
        });

        let body = json!({"model": "gpt-5.4-mini"});
        let (result, original, mapped) = apply_model_mapping(body, &provider);

        assert_eq!(result["model"], "gpt-5.4-mini");
        assert_eq!(original, Some("gpt-5.4-mini".to_string()));
        assert!(mapped.is_none());
    }

    #[test]
    fn test_subagent_model_preserved_with_one_m_suffix_before_default_fallback() {
        let mut provider = create_provider_with_mapping();
        provider.settings_config = json!({
            "env": {
                "ANTHROPIC_MODEL": "default-model",
                "CLAUDE_CODE_SUBAGENT_MODEL": "gpt-5.4-mini"
            }
        });

        let body = json!({"model": "gpt-5.4-mini[1M]"});
        let (result, original, mapped) = apply_model_mapping(body, &provider);

        assert_eq!(result["model"], "gpt-5.4-mini[1M]");
        assert_eq!(original, Some("gpt-5.4-mini[1M]".to_string()));
        assert!(mapped.is_none());
    }

    #[test]
    fn test_no_mapping_configured() {
        let provider = create_provider_without_mapping();
        let body = json!({"model": "claude-sonnet-4-5"});
        let (result, original, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "claude-sonnet-4-5");
        assert_eq!(original, Some("claude-sonnet-4-5".to_string()));
        assert!(mapped.is_none());
    }

    #[test]
    fn test_case_insensitive() {
        let provider = create_provider_with_mapping();
        let body = json!({"model": "Claude-SONNET-4-5"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "sonnet-mapped");
        assert_eq!(mapped, Some("sonnet-mapped".to_string()));
    }

    #[test]
    fn strips_one_m_suffix_before_upstream() {
        let body = json!({"model": "deepseek-v4-pro[1M]"});
        let result = strip_one_m_suffix_for_upstream_from_body(body);
        assert_eq!(result["model"], "deepseek-v4-pro");
    }

    #[test]
    fn strips_one_m_suffix_after_mapping() {
        let mut provider = create_provider_with_mapping();
        provider.settings_config = json!({
            "env": {
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "deepseek-v4-pro [1M]"
            }
        });

        let body = json!({"model": "claude-sonnet-4-6"});
        let (mapped, _, _) = apply_model_mapping(body, &provider);
        let result = strip_one_m_suffix_for_upstream_from_body(mapped);

        assert_eq!(result["model"], "deepseek-v4-pro");
    }

    #[test]
    fn keeps_model_without_one_m_suffix() {
        let body = json!({"model": "deepseek-v4-pro"});
        let result = strip_one_m_suffix_for_upstream_from_body(body);
        assert_eq!(result["model"], "deepseek-v4-pro");
    }

    // ========================================================================
    // 模型家族路由单测
    // ========================================================================

    fn create_provider_with_routes() -> Provider {
        let mut provider = create_provider_with_mapping();
        provider.meta = Some(crate::provider::ProviderMeta {
            model_family_routes: serde_json::from_value(json!({
                "sonnet": {
                    "name": "Sonnet Upstream",
                    "baseUrl": "https://sonnet.upstream.example/",
                    "apiKey": "sk-sonnet-route",
                    "apiKeyField": "ANTHROPIC_AUTH_TOKEN",
                    "apiFormat": "openai_chat",
                    "model": "deepseek-sonnet-pro",
                    "isFullUrl": true
                },
                "opus": {
                    "name": "Opus Upstream",
                    "baseUrl": "https://opus.upstream.example",
                    "model": "kimi-opus"
                },
                "subagent": {
                    "name": "Sub Upstream",
                    "baseUrl": "https://sub.upstream.example",
                    "model": "gpt-5.4-mini"
                }
            }))
            .expect("routes json"),
            ..Default::default()
        });
        provider
    }

    #[test]
    fn classify_model_role_recognizes_all_roles() {
        assert_eq!(classify_model_role("claude-sonnet-4-5"), Some(ModelRole::Sonnet));
        assert_eq!(
            classify_model_role("claude-opus-4-5[1M]"),
            Some(ModelRole::Opus)
        );
        assert_eq!(
            classify_model_role("claude-fable-5[1m]"),
            Some(ModelRole::Fable)
        );
        assert_eq!(classify_model_role("claude-haiku-4-5"), Some(ModelRole::Haiku));
        assert_eq!(classify_model_role("Claude-SONNET-4-6"), Some(ModelRole::Sonnet));
    }

    #[test]
    fn classify_model_role_unknown_returns_none() {
        assert_eq!(classify_model_role("gpt-5.4-mini"), None);
        assert_eq!(classify_model_role("deepseek-v4-pro"), None);
        assert_eq!(classify_model_role(""), None);
    }

    #[test]
    fn resolve_model_family_route_hits_family_role() {
        let provider = create_provider_with_routes();
        let body = json!({"model": "claude-sonnet-4-5"});
        let (role, route) =
            resolve_model_family_route(&provider, &body).expect("sonnet route hit");
        assert_eq!(role, ModelRole::Sonnet);
        assert_eq!(route.base_url, "https://sonnet.upstream.example/");
        assert_eq!(route.model.as_deref(), Some("deepseek-sonnet-pro"));
    }

    #[test]
    fn resolve_model_family_route_prefers_target_model_exact_match() {
        // 路由「目标模型」精确匹配优先于家族子串：请求模型 == 路由目标即命中。
        // 接管时 CC 端 _MODEL 已写入路由目标，请求模型即目标本身。
        // 带 [1M] 后缀也应命中（忽略后缀精确匹配）。
        let mut provider = create_provider_with_routes();
        let meta = provider.meta.as_mut().unwrap();
        meta.model_family_routes.get_mut("sonnet").unwrap().model =
            Some("claude-sonnet-5".to_string());
        let body = json!({"model": "claude-sonnet-5[1M]"});
        let (role, route) =
            resolve_model_family_route(&provider, &body).expect("target model exact hit");
        assert_eq!(role, ModelRole::Sonnet);
        assert_eq!(route.base_url, "https://sonnet.upstream.example/");
    }

    #[test]
    fn resolve_model_family_route_target_model_matches_non_family_model() {
        // 请求模型不含任何家族子串（如中转站的别名），但路由目标精确匹配
        // 仍能命中路由——这正是「目标模型即客户端模型名」要解决的核心场景
        // （CC 端 _MODEL 写入目标模型后，请求模型就是厂商别名，家族子串无法识别）。
        let mut provider = create_provider_with_routes();
        let meta = provider.meta.as_mut().unwrap();
        meta.model_family_routes.get_mut("sonnet").unwrap().model =
            Some("relay-claude-s5".to_string());
        let body = json!({"model": "relay-claude-s5"});
        let (role, route) =
            resolve_model_family_route(&provider, &body).expect("target model hit");
        assert_eq!(role, ModelRole::Sonnet);
        assert_eq!(route.base_url, "https://sonnet.upstream.example/");
    }

    #[test]
    fn resolve_model_family_route_empty_base_url_is_invalid() {
        let mut provider = create_provider_with_routes();
        let meta = provider.meta.as_mut().unwrap();
        meta.model_family_routes.insert(
            "haiku".to_string(),
            crate::provider::ModelFamilyRoute {
                base_url: "   ".to_string(),
                ..Default::default()
            },
        );
        let body = json!({"model": "claude-haiku-4-5"});
        assert!(resolve_model_family_route(&provider, &body).is_none());
    }

    #[test]
    fn resolve_model_family_route_subagent_detection_by_target() {
        let provider = create_provider_with_routes();
        let body = json!({"model": "gpt-5.4-mini"});
        let (role, route) =
            resolve_model_family_route(&provider, &body).expect("subagent route hit");
        assert_eq!(role, ModelRole::Subagent);
        assert_eq!(route.base_url, "https://sub.upstream.example");
    }

    #[test]
    fn resolve_model_family_route_subagent_detection_by_detection_key() {
        let mut provider = create_provider_with_routes();
        // 检测键与路由目标不同：命中检测键也应路由
        let meta = provider.meta.as_mut().unwrap();
        meta.model_family_routes
            .get_mut("subagent")
            .unwrap()
            .model = Some("gpt-5.4-pro".to_string());
        // 保留 detection key 在 env 中
        provider.settings_config["env"]["CLAUDE_CODE_SUBAGENT_MODEL"] =
            json!("gpt-5.4-mini");
        let body = json!({"model": "gpt-5.4-mini"});
        let (role, _) =
            resolve_model_family_route(&provider, &body).expect("subagent detection hit");
        assert_eq!(role, ModelRole::Subagent);
    }

    #[test]
    fn derive_routed_provider_rewrites_env_and_meta() {
        let provider = create_provider_with_routes();
        let (role, route) = resolve_model_family_route(
            &provider,
            &json!({"model": "claude-sonnet-4-5"}),
        )
        .expect("route");
        let routed = derive_routed_provider(&provider, role, &route);

        assert_eq!(
            routed.settings_config["env"]["ANTHROPIC_BASE_URL"],
            json!("https://sonnet.upstream.example")
        );
        assert_eq!(
            routed.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
            json!("sk-sonnet-route")
        );
        assert_eq!(
            routed.settings_config["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            json!("deepseek-sonnet-pro")
        );
        assert_eq!(
            routed.meta.as_ref().unwrap().api_format.as_deref(),
            Some("openai_chat")
        );
        // 路由的 isFullUrl 必须传递到派生 Provider 的 meta.is_full_url——
        // forward 的 is_full_url 判定读 meta.is_full_url，缺失会导致前端已保存的
        // "完整端点模式"开关不生效。
        assert_eq!(
            routed.meta.as_ref().unwrap().is_full_url,
            Some(true)
        );
        // 原 Provider 未被修改（immutable）：ANTHROPIC_BASE_URL 原本不存在
        assert!(provider.settings_config["env"]
            .get("ANTHROPIC_BASE_URL")
            .is_none());
        assert!(provider.settings_config["env"]
            .get("ANTHROPIC_AUTH_TOKEN")
            .is_none());
    }

    #[test]
    fn derive_routed_provider_clears_stale_url_hint_fields() {
        let mut provider = create_provider_with_routes();
        // 顶层 URL 提示字段
        provider.settings_config["base_url"] = json!("https://main.upstream.example");
        provider.settings_config["baseURL"] = json!("https://main.upstream.example/v1");
        provider.settings_config["baseUrl"] = json!("https://main.upstream.example/");
        provider.settings_config["apiEndpoint"] = json!("https://main.upstream.example/messages");

        let (role, route) = resolve_model_family_route(
            &provider,
            &json!({"model": "claude-sonnet-4-5"}),
        )
        .expect("route");
        let routed = derive_routed_provider(&provider, role, &route);

        let obj = routed.settings_config.as_object().unwrap();
        assert!(!obj.contains_key("base_url"));
        assert!(!obj.contains_key("baseURL"));
        assert!(!obj.contains_key("baseUrl"));
        assert!(!obj.contains_key("apiEndpoint"));
    }

    #[test]
    fn derive_routed_provider_empty_model_passthrough_clears_role_key_and_default() {
        let mut provider = create_provider_with_routes();
        let mut meta = provider.meta.as_mut().unwrap().clone();
        meta.model_family_routes.insert(
            "opus".to_string(),
            crate::provider::ModelFamilyRoute {
                name: "Passthrough".to_string(),
                base_url: "https://passthrough.example".to_string(),
                ..Default::default()
            },
        );
        let opus_provider = Provider {
            meta: Some(meta),
            ..provider
        };

        let (role, route) = resolve_model_family_route(
            &opus_provider,
            &json!({"model": "claude-opus-4-5"}),
        )
        .expect("route");
        let routed = derive_routed_provider(&opus_provider, role, &route);

        let env = routed.settings_config["env"].as_object().unwrap();
        // 角色键与 ANTHROPIC_MODEL 都被删除（透传原模型）
        assert!(!env.contains_key("ANTHROPIC_DEFAULT_OPUS_MODEL"));
        assert!(!env.contains_key("ANTHROPIC_MODEL"));
        // base_url 仍改写为路由 URL
        assert_eq!(
            routed.settings_config["env"]["ANTHROPIC_BASE_URL"],
            json!("https://passthrough.example")
        );
    }

    #[test]
    fn derive_routed_provider_preserves_is_full_url_only_when_set() {
        // isFullUrl=false 的路由不得覆写继承自主 Provider 的 meta.is_full_url
        let mut provider = create_provider_with_mapping();
        provider.meta = Some(crate::provider::ProviderMeta {
            is_full_url: Some(true),
            model_family_routes: serde_json::from_value(json!({
                "opus": {
                    "name": "Opus Upstream",
                    "baseUrl": "https://opus.upstream.example",
                    "isFullUrl": false
                }
            }))
            .expect("routes json"),
            ..Default::default()
        });
        let (role, route) = resolve_model_family_route(
            &provider,
            &json!({"model": "claude-opus-4-5"}),
        )
        .expect("route");
        let routed = derive_routed_provider(&provider, role, &route);
        // 路由显式 isFullUrl=false 应覆盖继承值（前端保存即用户意图）
        assert_eq!(routed.meta.as_ref().unwrap().is_full_url, Some(false));

        // 路由未设置 isFullUrl 时保留继承自主 Provider 的 meta.is_full_url
        let provider2 = create_provider_with_routes();
        let (role2, route2) = resolve_model_family_route(
            &provider2,
            &json!({"model": "claude-sonnet-4-5"}),
        )
        .expect("route");
        // sonnet 路由已设 isFullUrl=true，派生后应为 true
        let routed2 = derive_routed_provider(&provider2, role2, &route2);
        assert_eq!(routed2.meta.as_ref().unwrap().is_full_url, Some(true));
    }

    #[test]
    fn derive_routed_provider_effort_special_case_follows_route_url() {
        // §9.2 的 xhigh 过度 clamp 避坑：effort 特判（如 DeepSeek 官方端点的
        // thinking-disabled effort 剥离）以 env.ANTHROPIC_BASE_URL 判定。这里
        // 断言派生 Provider 的 env.ANTHROPIC_BASE_URL 正确指向路由上游、且顶层
        // 旧 URL 提示字段被清除——effort 特判据此读到路由 URL 而非主 Provider 旧值。
        const DEEPSEEK_URL: &str = "https://api.deepseek.com/anthropic";

        let mut provider = create_provider_with_mapping();
        // 主 Provider 配置了 DeepSeek 官方端点，但路由把请求发往其他上游
        provider.settings_config = json!({
            "env": {
                "ANTHROPIC_BASE_URL": DEEPSEEK_URL,
                "ANTHROPIC_AUTH_TOKEN": "sk-main"
            },
            "base_url": DEEPSEEK_URL,
            "baseURL": DEEPSEEK_URL,
            "apiEndpoint": DEEPSEEK_URL
        });
        provider.meta = Some(crate::provider::ProviderMeta {
            model_family_routes: serde_json::from_value(json!({
                "sonnet": {
                    "name": "Other Upstream",
                    "baseUrl": "https://other.upstream.example",
                    "apiFormat": "anthropic"
                }
            }))
            .expect("routes json"),
            ..Default::default()
        });

        let (role, route) = resolve_model_family_route(
            &provider,
            &json!({"model": "claude-sonnet-4-5"}),
        )
        .expect("route");
        let routed = derive_routed_provider(&provider, role, &route);

        // 路由后 env.ANTHROPIC_BASE_URL 指向路由上游（非 DeepSeek）
        assert_eq!(
            routed.settings_config["env"]["ANTHROPIC_BASE_URL"],
            json!("https://other.upstream.example")
        );
        // 顶层旧 URL 提示字段已被清除：effort 特判不会读到 DeepSeek 旧值
        assert!(routed.settings_config.get("base_url").is_none());
        assert!(routed.settings_config.get("baseURL").is_none());
        assert!(routed.settings_config.get("apiEndpoint").is_none());

        // 反向：路由到 DeepSeek 官方端点 → env.ANTHROPIC_BASE_URL 指向 DeepSeek，
        // effort 特判据此正确触发
        let mut meta = provider.meta.unwrap().clone();
        meta.model_family_routes.insert(
            "opus".to_string(),
            crate::provider::ModelFamilyRoute {
                name: "DeepSeek".to_string(),
                base_url: DEEPSEEK_URL.to_string(),
                ..Default::default()
            },
        );
        let provider_ds = crate::provider::Provider {
            meta: Some(meta),
            ..provider
        };
        let (role_ds, route_ds) = resolve_model_family_route(
            &provider_ds,
            &json!({"model": "claude-opus-4-5"}),
        )
        .expect("route");
        let routed_ds = derive_routed_provider(&provider_ds, role_ds, &route_ds);
        assert_eq!(
            routed_ds.settings_config["env"]["ANTHROPIC_BASE_URL"],
            json!(DEEPSEEK_URL)
        );
        assert!(routed_ds.settings_config.get("base_url").is_none());
    }

    #[test]
    fn derive_routed_provider_subagent_keeps_detection_key() {
        let mut provider = create_provider_with_routes();
        // subagent 路由未指定 model（透传），检测键应保留
        let mut meta = provider.meta.as_mut().unwrap().clone();
        meta.model_family_routes.insert(
            "subagent".to_string(),
            crate::provider::ModelFamilyRoute {
                name: "Sub".to_string(),
                base_url: "https://sub2.example".to_string(),
                ..Default::default()
            },
        );
        provider.settings_config["env"]["CLAUDE_CODE_SUBAGENT_MODEL"] =
            json!("gpt-5.4-mini");
        let sub_provider = Provider {
            meta: Some(meta),
            ..provider
        };

        let (role, route) = resolve_model_family_route(
            &sub_provider,
            &json!({"model": "gpt-5.4-mini"}),
        )
        .expect("route");
        let routed = derive_routed_provider(&sub_provider, role, &route);

        // 检测键不被删除
        assert_eq!(
            routed.settings_config["env"]["CLAUDE_CODE_SUBAGENT_MODEL"],
            json!("gpt-5.4-mini")
        );
    }

    #[test]
    fn model_mapping_subagent_target_applied() {
        // 检测键命中，subagent 路由目标与请求模型相同 → 透传（不改写）
        let mut provider = create_provider_with_routes();
        provider.settings_config["env"]["CLAUDE_CODE_SUBAGENT_MODEL"] =
            json!("gpt-5.4-mini");
        let body = json!({"model": "gpt-5.4-mini"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "gpt-5.4-mini");
        assert!(mapped.is_none());
    }

    #[test]
    fn model_mapping_subagent_target_rewrites_to_target() {
        // 检测键命中，subagent 路由目标与请求模型不同 → 改写为目标模型
        let mut provider = create_provider_with_routes();
        provider.settings_config["env"]["CLAUDE_CODE_SUBAGENT_MODEL"] =
            json!("gpt-5.4-mini");
        provider.meta.as_mut().unwrap().model_family_routes
            .get_mut("subagent")
            .unwrap()
            .model = Some("gpt-5.4-pro".to_string());
        let body = json!({"model": "gpt-5.4-mini"});
        let (result, original, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "gpt-5.4-pro");
        assert_eq!(original, Some("gpt-5.4-mini".to_string()));
        assert_eq!(mapped, Some("gpt-5.4-pro".to_string()));
    }

    #[test]
    fn model_mapping_subagent_no_route_preserves_model() {
        // 无 subagent 路由时保留原模型（向后兼容，原行为）
        let mut provider = create_provider_with_mapping();
        provider.settings_config = json!({
            "env": {
                "ANTHROPIC_MODEL": "default-model",
                "CLAUDE_CODE_SUBAGENT_MODEL": "gpt-5.4-mini"
            }
        });
        let body = json!({"model": "gpt-5.4-mini"});
        let (result, _, mapped) = apply_model_mapping(body, &provider);
        assert_eq!(result["model"], "gpt-5.4-mini");
        assert!(mapped.is_none());
    }

    #[test]
    fn apply_model_mapping_on_routed_provider_changes_model() {
        let provider = create_provider_with_routes();
        let (role, route) = resolve_model_family_route(
            &provider,
            &json!({"model": "claude-sonnet-4-5"}),
        )
        .expect("route");
        let routed = derive_routed_provider(&provider, role, &route);
        let body = json!({"model": "claude-sonnet-4-5"});
        let (result, _, mapped) = apply_model_mapping(body, &routed);
        // 派生 Provider 的角色 env 键已被改写为路由目标模型
        assert_eq!(result["model"], "deepseek-sonnet-pro");
        assert_eq!(mapped, Some("deepseek-sonnet-pro".to_string()));
    }

    #[test]
    fn has_mapping_includes_subagent_target() {
        let mut provider = create_provider_with_routes();
        // 只有 subagent 路由 target 时，has_mapping 应为 true
        let mut meta = provider.meta.as_mut().unwrap().clone();
        meta.model_family_routes.clear();
        meta.model_family_routes.insert(
            "subagent".to_string(),
            crate::provider::ModelFamilyRoute {
                name: "Sub".to_string(),
                base_url: "https://sub3.example".to_string(),
                model: Some("gpt-5.4-mini".to_string()),
                ..Default::default()
            },
        );
        let sub_provider = Provider {
            settings_config: json!({"env": {}}),
            meta: Some(meta),
            ..provider
        };
        let mapping = ModelMapping::from_provider(&sub_provider);
        assert!(mapping.has_mapping());
        assert_eq!(mapping.subagent_target.as_deref(), Some("gpt-5.4-mini"));
    }
}
