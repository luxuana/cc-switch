# 模型家族路由功能设计文档

> 状态：已实现（v3.19.1 基线落地）· 版本：v2 · 日期：2026-08-01 · 落地日期：2026-08-03

## 1. 功能概述

在 **Claude Code CLI**（`appId === "claude"`）的供应商编辑页面中，为模型映射表格新增"模型家族路由"能力：让 **Sonnet / Opus / Fable / Haiku / Subagent** 五个模型角色各自独立路由到一个上游厂商（各自拥有独立的 base_url、API Key、协议格式），而不是所有角色都走同一 Provider。

这本质上把当前"单 Provider 单上游"的模型映射，扩展为"单 Provider 多上游"的角色级路由。

### 1.1 用户交互（需求原文）

1. **模型映射表格**：每个模型行最右侧新增一个「配置路由」按钮。
2. 点击按钮 → 弹出「编辑供应商页面」窗口：**与内置供应商编辑页面一致**（供应商名称 / Base URL / API Key / 认证字段 / API 格式 / **模型映射区域**），但**没有「配置 JSON」选项**。
   - **模型映射区域为简化版**：只针对当前路由角色（`role` 由上一页点击哪一行传入，点击 Sonnet 行即 Sonnet、点击 Opus 行即 Opus），渲染一个「目标模型」输入框，外加「一键设置」和「获取模型列表」按钮——**不是**主编辑页的五行模型映射表格。
3. 配置好保存后返回上一页：
   - 「实际请求模型」输入框显示 **配置的模型厂商名称**（即路由供应商的 name）。
   - 「配置路由」按钮显示**一圈蓝色光晕边框**（表示已配置）。

## 2. 现状分析（调研结论）

### 2.1 模型映射现状

- **前端**：`src/components/providers/forms/ClaudeFormFields.tsx` 的 `modelRoleRows`（行 604-655）定义五行角色，每行有 `modelField`（如 `ANTHROPIC_DEFAULT_SONNET_MODEL`）与 `displayNameField`。表格 grid 为 `md:grid-cols-[120px_1fr_minmax(0,1fr)_104px]`（角色 / 显示名称 / 实际请求模型 / 1M）。
- **后端**：`src-tauri/src/proxy/model_mapper.rs` 的 `ModelMapping::from_provider` 从 `settings_config.env` 读取各角色模型，`map_model`（行 69-113）按模型名子串（`fable/haiku/opus/sonnet`）匹配角色，替换 `body["model"]`。

**关键限制**：所有角色请求最终走**同一** Provider 的 `base_url + auth + apiFormat`。

### 2.2 转发链路（后端）

```
HTTP handler → RequestContext::new（选 Provider）
  → RequestForwarder::forward_with_retry → forward_with_retry_inner
    → RequestForwarder::forward (forwarder.rs:1115)
      ├─ adapter.extract_base_url(provider)          (1127) ← base_url
      ├─ apply_model_mapping(body, provider)          (1167) ← 模型名替换
      ├─ strip [1m] / Copilot 归一化                  (1171-1193)
      ├─ 协议转换 needs_transform → transform          (1344-1522)
      ├─ adapter.build_url                            (1389-1402)
      ├─ adapter.extract_auth + get_auth_headers       (1614-1766) ← 认证头
      └─ 发送请求                                     (2216-2271)
```

**插入点**：在 `forward` 方法最前（`extract_base_url` 之前）解析"模型家族路由"：若请求 `body.model` 命中某个已配置路由的角色，则**派生一个 Provider**（改 `settings_config.env` 的 base_url/auth、`meta.api_format`），用派生 Provider 继续走完整个 forward 管道。**后续所有协议转换/认证/URL 构建代码零改动**（它们已按 Provider 独立抽象）。

### 2.3 协议兼容

`get_claude_api_format`（claude.rs:38-94）按 `meta.api_format` 判定 `anthropic / openai_chat / openai_responses / gemini_native`；`ProviderType` 决定 base_url 与认证策略。**每个 Provider 独立判定协议**，因此"Sonnet→Anthropic 上游、Opus→OpenAI 上游"不需要新协议层。

### 2.4 最接近的既有实现

`ClaudeDesktopProviderForm`（前端）把"角色→模型路由表"写入 `ProviderMeta.claudeDesktopModelRoutes`；后端 `map_proxy_request_model`（claude_desktop_config.rs:686）读取该表按角色映射模型。**但其路由只映射模型名，不携带独立 base_url/auth**。本功能是它的"完整供应商路由"升级。

## 3. 数据模型

### 3.1 后端（Rust，`src-tauri/src/provider.rs`）

在 `ProviderMeta` 新增字段：

```rust
/// 模型家族路由：Claude Code 各模型角色 → 独立上游供应商。
/// 仅存于 DB meta 列（~/.cc-switch/config.json），不写入 live 配置。
#[serde(
    default,
    rename = "modelFamilyRoutes",
    skip_serializing_if = "HashMap::is_empty"
)]
pub model_family_routes: HashMap<String, ModelFamilyRoute>,
```

新结构体 `ModelFamilyRoute`：

```rust
/// 单个模型角色的独立上游供应商配置
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ModelFamilyRoute {
    /// 供应商名称（显示在「实际请求模型」输入框）
    pub name: String,
    /// 上游 base_url
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    /// API Key
    #[serde(rename = "apiKey", skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// 认证字段名（ANTHROPIC_AUTH_TOKEN / ANTHROPIC_API_KEY）
    #[serde(rename = "apiKeyField", skip_serializing_if = "Option::is_none")]
    pub api_key_field: Option<String>,
    /// API 格式（anthropic / openai_chat / openai_responses / gemini_native）
    #[serde(rename = "apiFormat", skip_serializing_if = "Option::is_none")]
    pub api_format: Option<String>,
    /// 该角色在此上游的目标模型名（弹窗「目标模型」输入框，缺省则透传请求模型）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 是否将 base_url 视为完整端点（代理不拼接路径）
    #[serde(rename = "isFullUrl", skip_serializing_if = "Option::is_none")]
    pub is_full_url: Option<bool>,
}
```

> 说明：`model_family_routes` 的 key 固定为角色名 `"sonnet" | "opus" | "fable" | "haiku" | "subagent"`。路由配置与角色模型映射（`env.ANTHROPIC_DEFAULT_*_MODEL`）**相互独立**：角色模型映射管"模型名替换"，路由管"请求发往哪个上游"。

### 3.2 前端（TS，`src/types.ts`）

在 `ProviderMeta` 新增：

```ts
// 模型家族路由：Claude Code 各模型角色 → 独立上游供应商
modelFamilyRoutes?: Record<
  "sonnet" | "opus" | "fable" | "haiku" | "subagent",
  ModelFamilyRoute
>;
```

新接口 `ModelFamilyRoute`（字段与后端 serde 名一致）：

```ts
export interface ModelFamilyRoute {
  name: string;                 // 供应商名称（回显到「实际请求模型」输入框）
  baseUrl: string;              // 上游 base_url
  apiKey?: string;              // API Key
  apiKeyField?: ClaudeApiKeyField;  // ANTHROPIC_AUTH_TOKEN | ANTHROPIC_API_KEY
  apiFormat?: ClaudeApiFormat;  // anthropic | openai_chat | openai_responses | gemini_native
  model?: string;               // 该角色在此上游的目标模型
  isFullUrl?: boolean;          // 完整端点模式
}
```

> 说明：`model_family_routes` 的 key 固定为角色名 `"sonnet" | "opus" | "fable" | "haiku" | "subagent"`。每个路由只为**一个**角色配置一个目标模型（`model`），与主 Provider 的完整角色模型映射（`env.ANTHROPIC_DEFAULT_*_MODEL`）解耦：路由管"请求发往哪个上游 + 目标模型名"，主映射管"非路由请求的模型名替换"。

## 4. 后端实现方案

### 4.1 核心：派生 Provider

新增模块函数（放在 `src-tauri/src/proxy/model_mapper.rs` 或新建 `src-tauri/src/proxy/model_family_route.rs`）：

```rust
/// 从 Provider 提取模型家族路由表
pub fn model_family_routes(provider: &Provider) -> &HashMap<String, ModelFamilyRoute>;

/// 根据请求模型名判定角色（复用 map_model 的角色匹配逻辑，抽出共享函数）
pub fn classify_model_role(model: &str) -> Option<ModelRole>;

/// 命中路由时派生 Provider：改写 env base_url/auth + meta.api_format
pub fn derive_routed_provider(
    provider: &Provider,
    route: &ModelFamilyRoute,
) -> Provider;
```

**`derive_routed_provider` 具体做法**（Immutable：返回新 Provider，不修改原对象）：

```rust
let mut routed = provider.clone();
let mut env = routed.settings_config["env"].clone(); // 若无则建 {}
env["ANTHROPIC_BASE_URL"] = json!(route.base_url.trim_end_matches('/'));
let key_field = route.api_key_field.as_deref().unwrap_or("ANTHROPIC_AUTH_TOKEN");
if let Some(key) = &route.api_key { env[key_field] = json!(key); }
routed.settings_config["env"] = env;
// 协议格式：写入 meta.api_format（SSOT）
let mut meta = routed.meta.clone().unwrap_or_default();
meta.api_format = route.api_format.clone();
routed.meta = Some(meta);
// 角色模型映射：若路由指定了 model，写入对应 env 键，令后续 apply_model_mapping 生效
```

**角色 → env 键映射**：

| 角色 | env 键 |
|---|---|
| sonnet | `ANTHROPIC_DEFAULT_SONNET_MODEL` |
| opus | `ANTHROPIC_DEFAULT_OPUS_MODEL` |
| fable | `ANTHROPIC_DEFAULT_FABLE_MODEL` |
| haiku | `ANTHROPIC_DEFAULT_HAIKU_MODEL` |
| subagent | `CLAUDE_CODE_SUBAGENT_MODEL` |

### 4.2 转发集成（`forwarder.rs`）

在 `RequestForwarder::forward` 方法内、`extract_base_url`（行 1127）**之前**插入：

```rust
// 模型家族路由：命中已配置角色路由时，派生独立上游 Provider
let provider = resolve_model_family_route(provider, &body)
    .map(|(route)| derive_routed_provider(provider, &route))
    .unwrap_or_else(|| provider.to_owned());
```

其中 `resolve_model_family_route`（实际实现为三步匹配，返回 `(ModelRole, ModelFamilyRoute)` owned 元组）：

```rust
fn resolve_model_family_route(provider: &Provider, body: &Value)
    -> Option<(ModelRole, ModelFamilyRoute)>
{
    // 1. 路由「目标模型」精确匹配（对所有角色，含 subagent 目标）：
    //    请求 body.model（忽略 [1M] 与大小写）== 某路由的 model 即命中。
    //    接管时 CC 端 _MODEL 已写入路由目标，请求模型即目标本身——稳定命中；
    //    厂商别名（不含家族子串）也能命中。
    // 2. 家族子串匹配：classify_model_role(model) 命中已配置路由的角色。
    // 3. subagent 检测键匹配：请求模型 == env.CLAUDE_CODE_SUBAGENT_MODEL。
    // 路由 base_url 为空视为无效（§8.3 判空）。
}
```

> **客户端显示名 / 能力判定跟随目标模型**：接管时（`apply_claude_takeover_fields_for_provider`）若某角色路由配置了 `model`，`ANTHROPIC_DEFAULT_*_MODEL` 写入该目标模型（替代固定接管别名如 `claude-sonnet-4-6`）。Claude Code 显示它并按它做能力判定（如 xhigh 思考支持）——用户填什么目标模型，客户端就按什么判定；转发前 `[1M]` 后缀由 `strip_one_m_suffix_for_upstream` 剥离。目标模型缺省时保持固定接管别名（向后兼容）。

> 注意：`forward` 参数中的 `provider` 目前是 `&Provider`（借引用）。集成时需把后续代码对 `provider` 的使用切换为局部 `let provider = ...to_owned()`（forward 内部已有大量 `provider.meta.as_ref()` 等只读用法，`Provider` 是 `Clone`，成本可忽略）。需要子代理通读 `forward` 方法体确认所有 `provider` 引用点，统一改为局部变量。

### 4.3 无路由时的行为

`model_family_routes` 为空 / 模型不命中任何已配置角色 → 走现有逻辑，行为与现在完全一致（向后兼容）。

### 4.4 测试（Rust 单测）

在 `model_mapper.rs` 或新模块加 `#[cfg(test)]`：

- `classify_model_role` 对各角色名识别正确（含 `[1M]` 后缀、大小写、未知模型返回 None）。
- `derive_routed_provider` 正确改写 env base_url/auth、meta.api_format、角色模型 env 键。
- `apply_model_mapping` 在派生 Provider 上生效（路由的 model 替换请求模型）。
- 端到端 `forward` 集成测试：命中 Sonnet 路由 → base_url 指向路由上游 + 请求模型为路由 model。

## 5. 前端实现方案

### 5.1 模型映射行新增「配置路由」列

`src/components/providers/forms/ClaudeFormFields.tsx`：

- 表格 grid 从 4 列改为 5 列：
  - 表头（行 973）：`md:grid-cols-[120px_1fr_minmax(0,1fr)_104px_96px]`，新增表头 `配置路由`。
  - 行（行 1004）：同样 grid，在 1M Checkbox 后新增一个 `<div className="flex h-9 items-center">` 放「配置路由」按钮。
- 新增 props（向 `ProviderFormFull` 冒泡）：

```ts
// 当前 Provider 的模型家族路由（来自 initialData.meta.modelFamilyRoutes）
modelFamilyRoutes?: Record<ModelRole, ModelFamilyRoute>;
// 点击「配置路由」按钮回调（role + 当前路由配置或 null）
onConfigureRoute: (role: ModelRole, route: ModelFamilyRoute | null) => void;
```

- 每行按钮状态：
  - **未配置**：`Button variant="outline" size="sm"`，文字「配置路由」。
  - **已配置**：按钮 `className` 加蓝色光晕 `border-blue-500/60 shadow-[0_0_10px_rgba(59,130,246,0.35)]`，文字可显示厂商名或仍为「配置路由」。
- 「实际请求模型」输入框（`renderModelInput`，行 1033-1045）在**已配置路由**时显示 `route.name`（供应商名称），仍可点击「配置路由」查看详情。显示供应商名称而非模型名——与需求一致。

### 5.2 「编辑供应商页面」弹窗

**新增组件** `src/components/providers/forms/ModelFamilyRouteEditor.tsx`：

- 容器：`FullScreenPanel`（与 EditProviderDialog 一致）。**接收 `role: ModelRole` prop**——由上一页（`ClaudeFormFields` 的 modelRoleRows）点击哪一行传入，标题随角色动态生成，如「配置 Fable 路由」「配置 Opus 路由」。
- **内容**：复用内置供应商编辑表单的字段，但**不含「配置 JSON」**（不渲染 `CommonConfigEditor`）。需要向 `ProviderFormFull` 增加一个 `hideConfigJson?: boolean` prop 或新增一个精简表单组件。
  - **推荐**：复用 `ProviderForm` 的 `ProviderFormFull`，新增 `hideConfigJson` prop（在行 2451-2596 的配置编辑器 switch 处包一层 `!hideConfigJson && (...)`）。这样"与内置页面一致、只是没有配置 JSON"的诉求最省力达成。
- **表单字段**（`ClaudeFormFields` 相关子集）：
  - 供应商名称（`name`）
  - Base URL（`baseUrl`，含测速）
  - API Key（`apiKey`）+ 认证字段选择（`apiKeyField`）
  - API 格式（`apiFormat`：Anthropic / OpenAI Chat / OpenAI Responses / Gemini Native）
- **模型映射区域（简化版）**：与主编辑页模型映射表格不同，弹窗内**只针对当前路由角色（即传入的 `role`）**配置目标模型：

```
模型映射
当前路由只能为一个 Claude Code 模型角色选择一个目标模型。
[一键设置]  [获取模型列表]
──────────────────────────────────────────
{role}   目标模型  [ deepseek-v4-pro ]
```

  - 只渲染一行：角色名显示为**传入的 `role` 值**（点击 Sonnet 行进来显示 Sonnet，点击 Opus 行进来显示 Opus，不写死）+「目标模型」输入框（对应 `ModelFamilyRoute.model`）。
  - **「获取模型列表」**按钮：复用现有内部逻辑——`ClaudeFormFields` 的 `handleFetchModels`（行 287-314）用 `baseUrl + apiKey + isFullUrl + modelsUrl + customUserAgent` 调 `fetchModelsForConfig` 拉取模型列表填入 `fetchedModels`，`ModelInputWithFetch` 下拉可选。路由弹窗内复用同一 `fetchModelsForConfig`，用**弹窗内填写的 Base URL + API Key** 拉取该路由供应商的模型。
  - **「一键设置」**按钮：在「获取模型列表」成功拉取到模型后，把拉取结果一键填入目标模型（即 `ModelInputWithFetch` 选中某个模型 → 填入 `ModelFamilyRoute.model`）。若未拉取则提示先填写 Base URL + API Key 并获取模型列表。
  - **「清除路由」**按钮：弹窗底部提供清除入口（`variant="ghost"` / `destructive`），点击后确认清除该角色路由，返回未配置态（按钮取消蓝色光晕、输入框恢复显示模型名）。
- **保存**：构造 `ModelFamilyRoute`，通过 `onSave(role, route)` 回调写回父级 state，关闭弹窗。
- **打开方式**：在 `ProviderFormFull` 持有 `editingRoute: { role, route } | null` state，传给 `ClaudeFormFields.onConfigureRoute` 打开。编辑已配置路由时传入现有 `route` 回显。

### 5.3 状态管理

- `ProviderFormFull`（`src/components/providers/forms/ProviderForm.tsx`）：
  - 新增 state：`modelFamilyRoutes: Record<ModelRole, ModelFamilyRoute>`（初始化自 `initialData?.meta?.modelFamilyRoutes`）。
  - `performSubmit`（行 1540-1657 构造 `nextMeta`）中合并：`nextMeta.modelFamilyRoutes = modelFamilyRoutes`（仅当非空）。
- 保存数据流：`ClaudeFormFields.onConfigureRoute → ProviderFormFull 打开弹窗 → ModelFamilyRouteEditor 编辑保存 → 更新 modelFamilyRoutes state → ClaudeFormFields 收到新 props 刷新按钮/输入框显示 → performSubmit 提交 meta`。

### 5.4 生效前提提示

模型家族路由只在 **Claude 本地路由（代理接管）开启**时生效（本地代理按角色分流到不同上游）。在模型映射区加一条提示（`text-xs text-muted-foreground`）：「模型家族路由需在 Claude 本地路由开启时生效」，避免用户配置后不生效的困惑。可复用 `ClaudeDesktopRouteToggle` 展示的路由状态。

### 5.5 蓝色光晕样式

复用现有视觉语言（`ProviderCard.tsx` 行 306）：

```
border-blue-500/60 shadow-[0_0_10px_rgba(59,130,246,0.35)]
```

「配置路由」按钮已配置时加此 class；输入框显示供应商名称时可用同色 border 提示。

### 5.5 前端类型检查与测试

- `pnpm typecheck`（tsc --noEmit）通过。
- 新增/更新组件测试（`tests/components/`）：`ModelFamilyRouteEditor` 保存回调、已配置按钮光晕 class、输入框显示供应商名称。
- `pnpm test:unit` 通过。

## 6. 文件改动清单

### 后端（Rust）

| 文件 | 改动 |
|---|---|
| `src-tauri/src/provider.rs` | `ProviderMeta` 新增 `model_family_routes` 字段；新增 `ModelFamilyRoute` 结构体；`ModelRole` 枚举或常量 |
| `src-tauri/src/proxy/model_mapper.rs` | 抽 `classify_model_role` 共享函数；加 `derive_routed_provider`、`resolve_model_family_route`；角色→env 键映射常量；单测 |
| `src-tauri/src/proxy/forwarder.rs` | `forward` 开头插入路由解析；`provider` 改为局部 owned；集成测试 |

### 前端（TS/TSX）

| 文件 | 改动 |
|---|---|
| `src/types.ts` | `ProviderMeta` 加 `modelFamilyRoutes`；新增 `ModelFamilyRoute` 接口 |
| `src/components/providers/forms/ClaudeFormFields.tsx` | 新增一列「配置路由」按钮；新 props；已配置态显示/样式 |
| `src/components/providers/forms/ModelFamilyRouteEditor.tsx` | **新组件**：FullScreenPanel + 供应商表单（无配置 JSON） |
| `src/components/providers/forms/ProviderForm.tsx` | `hideConfigJson` prop；`modelFamilyRoutes` state；`performSubmit` 合并 meta；打开弹窗 |
| `tests/components/` | 新增/更新测试 |

### 文档

| 文件 | 改动 |
|---|---|
| `docs/user-manual/zh/2-providers/2.1-add.md`（或路由相关章节） | 补充模型家族路由说明 |

## 7. 任务拆分（子代理执行策略）

按用户要求"先出设计文档再拆任务"，本设计文档审核通过后按以下顺序执行：

### 阶段 A：后端（任务 #3）
- **难度**：高（涉及 `forward` 长方法改造、Provider 借用/所有权、角色识别）
- **思考程度**：Sonnet（xhigh）
- **范围**：Rust 全部改动 + 单测 + `cargo test` 通过
- **依赖**：无（可立即开始）

### 阶段 B：前端（任务 #4 + #5）
- **难度**：中高（表单复用、弹窗、状态管理、样式）
- **思考程度**：Sonnet（high）
- **范围**：types.ts + ClaudeFormFields + ModelFamilyRouteEditor + ProviderForm + 测试
- **依赖**：阶段 A 完成（保证 `ModelFamilyRoute` 字段名与后端 serde 名一致，避免契约漂移）

### 阶段 C：验证（任务 #6）
- **难度**：低
- **思考程度**：Haiku（medium）
- **范围**：全量 `cargo test` + `pnpm typecheck` + `pnpm test:unit`；抽查前后端字段契约一致性

> 成本/速度策略：后端先行、前端串行，避免双代理并行导致的字段契约不一致返工；子代理思考程度按任务难度分档（Sonnet xhigh / Sonnet high / Haiku medium）。

## 8. 风险与注意点

1. **`forward` 方法巨大（1115-2327）**：改 `provider` 从 `&Provider` 到局部 owned 需通读所有引用点，子代理需逐处确认，避免漏改。
2. **模型映射与路由的交互**：路由的 `model` 字段会写回角色 env 键，需确保 `apply_model_mapping` 与 `derive_routed_provider` 顺序正确（先派生再映射）。
3. **路由配置的 base_url 判空**：空 base_url 视为未配置，避免落入空地址。
4. **前后端字段名契约**：`modelFamilyRoutes` / `baseUrl` / `apiKey` / `apiKeyField` / `apiFormat` / `isFullUrl` 的 serde 名与 TS 字段必须一致，阶段 B 依赖阶段 A 的最终实现。
5. **向后兼容**：未配置路由的 Provider 行为完全不变。
6. **代理接管模式**：路由仅在代理接管（Claude 路由开启）时生效；非接管模式写 live 配置时，路由信息存 meta 不落 live，天然安全。

## 9. 实现记录（v2，与初始设计的偏差与专项审查修复）

> 本节内容来自**此前完成过本功能实现的项目**沉淀的经验：§9.1 为当时实现的实际偏差，§9.2-9.3 为专项审查发现的缺陷与修复。当前代码库（干净上游 v3.19.1）**尚未实现本功能**，落地时须把本节作为避坑清单**逐项落实**，不得跳过。

### 9.1 前端实现偏差

- **路由编辑弹窗用独立 `ModelFamilyRouteEditor` 组件（`Dialog` 模态弹窗）**，而非 §5.2 推荐的"复用 `ProviderFormFull` + `hideConfigJson`"。理由：主表单 2000+ 行（OAuth/预设/定价/通用配置等大量状态），跑"路由模式"需隐藏大半逻辑，回归风险高；专用组件复用共享叶子组件（`ApiKeySection`/`EndpointField`/`ModelInputWithFetch`/`Select`），视觉与内置页一致且零风险。
- **弹窗用居中 `Dialog`（z-[110]）**，非 §5.2 的 `FullScreenPanel`。用户实测反馈"不是弹窗"，已改。两个 `SelectContent` 需显式 `z-[120]` 浮于弹窗之上（沿用 `ModelsDevPickerDialog.tsx` 的先例），否则下拉被 z-[110] 弹窗遮住不可见。
- **`声明支持 1M` 复选框与目标模型解耦**：`targetModel` 存纯模型名、`targetUsesOneM` 独立布尔 state。初版把 `[1M]` 内嵌进模型字符串，复用 `setClaudeOneMMarker`，但该函数对空串直接返回 `""`，导致目标模型为空时复选框"点不动"。保存时按 `targetUsesOneM` 拼接 `[1M]` 后缀。
- **模型映射区排版**：按用户给的结构——「模型映射」标题、提示文字 + 「一键设置/获取模型列表」按钮同行右对齐、单行 `角色 | 目标模型 | 声明支持 1M`。

### 9.2 后端专项审查修复（审查子代理：通过 8 / 问题 5）

- **[HIGH] subagent 路由的 `model` 从未被应用**：`CLAUDE_CODE_SUBAGENT_MODEL` 在 `map_model` 中是「检测键」（识别 subagent 请求），`derive_routed_provider` 初版把路由 model 写进该键导致检测失效、映射永不生效。修复：`ModelMapping` 新增 `subagent_target`（从 `meta.model_family_routes["subagent"].model` 读取）；`derive_routed_provider` 对 subagent 角色**不改写检测键**；`map_model` subagent 分支命中时返回 `subagent_target`（无路由则保留原模型，向后兼容）。
- **[MEDIUM] 路由未指定 model 时继承主 Provider 角色映射**（与「缺省则透传」冲突）：`derive_routed_provider` 改为在 route.model 为空时**删除该角色 env 键**，令路由请求透传原模型。
- **[MEDIUM] xhigh 过度 clamp**：`provider_supports_xhigh_reasoning_effort` 会读到继承自主 Provider 的顶层 `base_url`/`baseURL`/`apiEndpoint` 提示字段，导致枚举受限平台主 Provider 路由到支持 xhigh 的上游时被误降档。修复：`derive_routed_provider` 清除这些过时 URL 提示字段（真实 URL 已在 `env.ANTHROPIC_BASE_URL`）。`meta.provider_type` 继承导致的过度 clamp 保留（影响小、清除风险高）。
- **[LOW，接受] subagent 模型名含家族子串会被家族路由抢先命中**（`classify_model_role` 优先）；**[LOW，接受] 路由继承主 Key 时认证策略随 base_url 变化**。
- **关键交互验证通过**：xhigh clamp 检测的是**路由后 Provider**（`env.ANTHROPIC_BASE_URL` 已改写为路由 URL），主 anthropic + 路由到枚举受限平台能正确 clamp；`forward` 中 `provider` 阴影切换后所有引用点均用路由值。

### 9.3 第 2/3 轮专项审计（对抗验证 + 前端/安全/回归）

- **[HIGH，第 2 轮发现并修复] 路由未指定 model 时仍会落到默认兜底**：`derive_routed_provider` 删除角色 env 键后，`map_model` 角色分支落空会继续落到 `ANTHROPIC_MODEL`（`UniversalProvider::to_claude_provider` 必写），把路由请求改写成主 Provider 默认模型而非透传。修复：route.model 为空（非 subagent 角色）时同时删除派生 Provider 的 `ANTHROPIC_MODEL`。
- **[防御性，第 2 轮] `has_mapping()` 纳入 `subagent_target`**（仅 subagent 路由 model 存在时不应跳过映射）；URL 清理名单补 `baseUrl`。
- **[LOW，第 3 轮] 前端**：`performSubmit` 的 `cleanModelFamilyRoutes` 归一化空字符串可选字段（apiKey/apiKeyField/apiFormat/model/isFullUrl 空值不落盘，与编辑器口径一致）；路由编辑器 `console.warn` 不再打印原始错误（避免上游 base_url/响应体泄漏到 devtools）。
- **forwarder 端到端核对通过（第 2 轮）**：命中 sonnet 路由 → openai_chat 全链路（模型映射→[1M]剥离→`get_claude_api_format` 读路由 meta→协议转换+xhigh clamp→`extract_base_url` 路由 URL→`extract_auth` 路由 key→URL 构建）逐环节均用路由后 Provider；anthropic/openai_chat/openai_responses/gemini_native 各格式核对无问题。
- **回归通过（第 3 轮）**：thinking effort→budget 双向 round-trip 恒等（2048/8192/16384/24576 无掉档）；tool_choice 三处守卫 + xhigh clamp 在路由 Provider 下仍正确；apiKey 不落 live/日志。
- **待确认（第 2 轮，未改）**：`forward` 的路由解析对所有 app_type 无条件执行，Claude Desktop proxy 模式若同时配 `claudeDesktopModelRoutes` + `modelFamilyRoutes` 存在双重映射交互（功能面向 Claude Code CLI，Desktop 场景未门控）；gemini_native 路由的 `api_key_field` 默认 `ANTHROPIC_AUTH_TOKEN`，若用户未改为 Gemini 专用字段则 key 挂在 AUTH_TOKEN 名下发送。

## 10. 落地记录（2026-08-03，v3.19.1 基线实现）

本节记录本功能在干净上游 v3.19.1 上的实际实现、与设计文档的偏差，以及三轮跨端审计的结论。实现完成后本节内容为「已完成」事实，不再标注待实现。

### 10.1 实际实现偏差（相对 §3-§5 设计）

| 设计点 | 实际实现 | 原因 |
|---|---|---|
| §4.2 `resolve_model_family_route` 返回 `&'p ModelFamilyRoute` | 返回 `(ModelRole, ModelFamilyRoute)` owned 元组 | 需从 `meta.model_family_routes` 返回 owned 值，且 subagent 需额外匹配 Provider 检测键 |
| §4.1 `derive_routed_provider(provider, route)` | 显式带 `role: ModelRole` 参数 | 角色模型 env 键需要角色信息 |
| §4.2 `resolve` 按 `classify_model_role` 判定 | subagent 额外支持精确匹配（路由目标 + 检测键，忽略 [1M]） | 保证 subagent 路由真实可达（§9.2a） |
| §4.1 `map_model` subagent 分支 | 命中检测键时有 `subagent_target` 返回目标、无则保留原模型、请求等于 target 透传 | 向后兼容 + 路由模型改写 |
| §5.2 弹窗复用 `ProviderFormFull` + `hideConfigJson` | 独立 `ModelFamilyRouteEditor`（Dialog）复用共享叶子组件 | 主表单 2000+ 行，跑路由模式回归风险高（§9.1） |
| §5.5 全量测试 | 前端 typecheck + 602 测试、后端 model_mapper 35 / forwarder 69 / transform 341 / transform_responses 76 / claude 64 全绿 | 全量验证通过 |

### 10.2 思考程度透传决策（用户拍板，替代 §9.2 的 xhigh clamp 担忧）

- **决策**：Claude Code CLI 的思考程度（`output_config.effort` / `thinking`）在路由中**完全透传**，路由不做主动降级；上游自行决定是否接受。统一思考级别下所有模型全档位支持，无需按模型白名单过滤。
- **实现**：`transform.rs` / `transform_responses.rs` 的 Claude→OpenAI 转换路径去掉 `supports_reasoning_effort` 白名单，`resolve_reasoning_effort` 结果**无条件注入**。
- **语义**：`resolve_reasoning_effort` 为"翻译"而非"降级"——`output_config.effort` low/medium/high 1:1、`max`→`xhigh`（OpenAI 等义最高档）、未知值保守丢弃；`thinking` 回退 adaptive→xhigh、budget→low/medium/high。
- **边界（有意行为）**：
  - 非 reasoning 上游（gpt-4o/DeepSeek/Kimi 等）现在也会收到 `reasoning_effort` / `reasoning.effort`，严格校验参数的上游可能 400——**上游自行处理**，属预期范围。
  - Codex→Chat 路径（`map_reasoning_effort` 的 deepseek/low_high/openrouter clamp）**不受影响**，方向正交，仅在配置 `effort_value_mode` 时触发。
  - `supports_reasoning_effort` 函数保留，仍被 Codex→Chat 路径使用，非 dead code。
- **DeepSeek 特判不受影响**：`normalize_deepseek_thinking_disabled_strip_effort` 只在 anthropic passthrough 路径 + thinking disabled 时剥离 effort（DeepSeek 协议硬约束，非降级），且该路径不进入 OpenAI 转换，透传无法绕过。

### 10.3 三轮跨端审计结论（2026-08-03）

以「前端审后端」「后端审前端」「行为变更专项回归」三个独立审计子代理并行执行，主代理逐条独立核验。

**整体结论：前后端跨端语义贯通成立，无 HIGH 缺陷，无必须先修项。**

- **字段契约**：`ModelFamilyRoute` serde rename 与前端 TS 逐字段一致（name/baseUrl/apiKey/apiKeyField/apiFormat/model/isFullUrl）；空值省略语义一致；后端 `model_family_routes_serde_contract_matches_frontend` 契约测试佐证。
- **语义贯通**：baseUrl 全链（resolve 判空 → derive 写 env → extract_base_url 读 env）；apiKey 继承语义；apiFormat 未指定时保留继承；model 透传防兜底（删角色键 + 删 ANTHROPIC_MODEL）；isFullUrl 传递到 meta.is_full_url；subagent 检测键保护。
- **forward 全链 shadow**：路由解析后所有引用点（约 30 处）均用路由后 Provider；无路由时 `unwrap_or_else` 保持原 Provider 严格向后兼容。

**审计发现并修复的项：**

- **死 i18n key `modelRouteTargetModelLabel`**：4 个语言文件定义、编辑器未用（§9.1 排版用角色徽标替代「目标模型」标签）。已删除。
- **`isFullUrl` 前端显示 vs 后端继承不一致**：弹窗打开时 `initialRoute?.isFullUrl ?? false` 显示 false，但路由未设 isFullUrl 时后端继承主 Provider 的 `meta.is_full_url`。已加 `defaultIsFullUrl` prop 回退读主 Provider 继承值。
- **测试名与断言脱节**：`test_non_reasoning_model_no_reasoning_effort` 名字暗示"不注入"但断言已改。已改名反映新语义。

**审计确认、作为已知风险记录（不修代码）：**

- **Claude Desktop 双重路由未门控**：`resolve_model_family_route` 对所有 app_type 执行，`claudeDesktopModelRoutes` + `modelFamilyRoutes` 并存时未定义（设计 §9.3 已声明）。功能面向 Claude Code CLI。**建议未来门控 `matches!(app_type, AppType::Claude)`**。
- **Codex 场景未门控**：Codex 请求 model 含家族子串且 Provider 配了路由时会派生路由 Provider（低概率，Codex 模型名通常不含家族子串）。
- **gemini_native 路由 key 挂 AUTH_TOKEN**：用户未改认证字段时 key 写入 `env.ANTHROPIC_AUTH_TOKEN`，`extract_key` 能读到、`provider_type` 判定 Gemini 后走 `x-goog-api-key`，能正常工作但语义上 key 挂错 env 变量名（设计 §9.3 已列）。
- **subagent 模型名含家族子串被家族路由抢先命中**（设计 §9.2 [LOW, accepted]）。
- **window.confirm**：设计 §4.8 明确写 `window.confirm`，实现照做；与项目 `ConfirmDialog` 惯例不一致，属设计文档已知差异，未改（功能正确、可测试）。

### 10.4 验证基线（2026-08-03）

- 前端：`pnpm typecheck` 通过；`pnpm test:unit` 全量 87 文件 / 602 测试通过。
- 后端：`cargo test --lib` 2276 通过 / 1 失败 / 2 ignored。唯一失败 `update_current_claude_desktop_provider_syncs_profile_when_proxy_takeover_is_active` 为**环境端口冲突**（运行中的 CC Switch 占用 127.0.0.1:15721，测试需绑定该端口），与本次改动无关。
- 跨端契约测试：后端 `model_family_routes_serde_contract_matches_frontend` 通过。
