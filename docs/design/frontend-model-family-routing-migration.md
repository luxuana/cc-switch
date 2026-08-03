# 模型家族路由（前端）魔改实现说明

> 状态：已实现（v3.19.1 基线落地，2026-08-03）· 基准上游 v3.19.1
> 用途：在**干净上游源码**（`https://github.com/farion1231/cc-switch.git`，v3.19.1）上重新实现本前端魔改的完整参考。
> 本文档只覆盖**前端**改动（UI / 状态 / 提交）。后端 Rust 的配套实现见 `docs/design/model-family-routing.md` §3-§4。
> 注意：本文档为落地实现的规格说明。落地偏差与审计结论见主文档 §10。

---

## 1. 功能概述

在 **Claude Code CLI 供应商编辑页面**（`appId === "claude"`）的模型映射表格中，为五个模型角色（Sonnet / Opus / Fable / Haiku / Subagent）各自增加**独立上游供应商路由**：每个角色可单独配置 base_url / API Key / 认证字段 / API 格式 / 目标模型。

本质上把"单 Provider 单上游"的模型映射，扩展为"单 Provider 多上游"的角色级路由。路由配置**仅存于 DB meta 列**（`~/.cc-switch/config.json`），**不写入 live 配置**；实际分流由后端代理执行。

---

## 2. 数据模型（`src/types.ts`）

### 2.1 新增类型

在 `LocalProxyRequestOverrides` 之后（约第 168 行）插入：

```ts
// Claude Code 模型角色（模型家族路由的 key）
export type ModelRole = "sonnet" | "opus" | "fable" | "haiku" | "subagent";

// 单个模型角色的独立上游供应商配置（模型家族路由）。
// 字段名与后端 serde rename 严格一致；仅存于 DB meta 列（~/.cc-switch/config.json），不写入 live 配置。
export interface ModelFamilyRoute {
  // 供应商名称（回显到「实际请求模型」输入框）
  name: string;
  // 上游 base_url（必填非空，否则视为未配置路由）
  baseUrl: string;
  // 可选；留空则后端继承主供应商 Key
  apiKey?: string;
  // 认证字段名（ANTHROPIC_AUTH_TOKEN / ANTHROPIC_API_KEY）
  apiKeyField?: ClaudeApiKeyField;
  // API 格式（anthropic / openai_chat / openai_responses / gemini_native）
  apiFormat?: ClaudeApiFormat;
  // 该角色在此上游的目标模型（缺省则透传请求模型）
  model?: string;
  // 是否将 base_url 视为完整端点（代理不拼接路径）
  isFullUrl?: boolean;
}

// 模型家族路由映射：仅包含已配置的角色（与后端 HashMap 一致，允许任意子集）
export type ModelFamilyRouteMap = Partial<Record<ModelRole, ModelFamilyRoute>>;
```

> `ClaudeApiKeyField` 与 `ClaudeApiFormat` 为既有类型，`src/types.ts` 已有定义，直接复用。

### 2.2 `ProviderMeta` 新增字段

在 `githubAccountId` 之后（约第 256 行）插入：

```ts
  // 模型家族路由：Claude Code 各模型角色（sonnet/opus/fable/haiku/subagent）→ 独立上游供应商。
  // 仅存于 DB meta 列（~/.cc-switch/config.json），不写入 live 配置。
  modelFamilyRoutes?: ModelFamilyRouteMap;
```

---

## 3. 模型映射表格改造（`src/components/providers/forms/ClaudeFormFields.tsx`）

### 3.1 新增 import

```ts
import type {
  ...
  ModelRole,
  ModelFamilyRoute,
  ModelFamilyRouteMap,
} from "@/types";
```

### 3.2 Props 新增

`ClaudeFormFieldsProps` 接口末尾追加：

```ts
  // 模型家族路由（Claude Code 各角色 → 独立上游供应商）
  modelFamilyRoutes?: ModelFamilyRouteMap;
  onConfigureRoute?: (role: ModelRole, route: ModelFamilyRoute | null) => void;
  // 是否处于 Claude 本地路由（代理接管）模式
  isProxyTakeover?: boolean;
```

函数解构处同步追加这三个参数。

### 3.3 提示文案（模型映射区标题行下方）

在 `<p>{t("providerForm.modelMappingHint")}</p>` 之后新增：

```tsx
<p className="text-xs text-muted-foreground">
  {isProxyTakeover
    ? t("providerForm.modelRouteHintActive", {
        defaultValue: "模型家族路由已生效（Claude 本地路由接管中）",
      })
    : t("providerForm.modelRouteHint", {
        defaultValue: "模型家族路由需在 Claude 本地路由开启时生效",
      })}
</p>
```

### 3.4 表格从 4 列扩为 5 列

- **表头**：grid 类从 `md:grid-cols-[120px_1fr_minmax(0,1fr)_104px]` 改为 `md:grid-cols-[120px_1fr_minmax(0,1fr)_104px_96px]`，在「声明支持 1M」列后新增表头：

```tsx
<span>
  {t("providerForm.modelRouteHeader", { defaultValue: "配置路由" })}
</span>
```

- **行**：grid 类同样改为 5 列。`modelRoleRows.map((row) => { ... })` 内，在 `row.role` 处读取路由：

```tsx
const route = modelFamilyRoutes?.[row.role];
const isRouteConfigured = Boolean(route && route.name && route.baseUrl);
```

### 3.5 「实际请求模型」列：已配置路由时显示供应商名

将原 `renderModelInput(...)` 块替换为条件渲染：

```tsx
{isRouteConfigured ? (
  <div className="flex h-9 items-center rounded-md border border-blue-500/40 bg-blue-500/5 px-3 text-sm font-medium text-blue-500">
    {route!.name}
  </div>
) : (
  renderModelInput(
    row.inputId,
    modelBase,
    row.modelField,
    t("providerForm.modelPlaceholder", { defaultValue: "" }),
    (value) =>
      handleRoleModelChange(
        row,
        row.supportsOneM
          ? setClaudeOneMMarker(value, usesOneM)
          : stripClaudeOneMMarker(value),
      ),
  )
)}
```

### 3.6 「配置路由」按钮列（新增，在 1M 复选框之后）

```tsx
{/* 无 1M 复选框的行补空占位，保持「配置路由」按钮对齐到最后一列 */}
{!row.supportsOneM && <div />}
<div className="flex h-9 items-center">
  <Button
    type="button"
    variant="outline"
    size="sm"
    onClick={() => onConfigureRoute?.(row.role, route ?? null)}
    className={
      isRouteConfigured
        ? "w-full gap-1 border-blue-500/60 shadow-[0_0_10px_rgba(59,130,246,0.35)]"
        : "w-full gap-1"
    }
  >
    {isRouteConfigured && (
      <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-blue-500" />
    )}
    <span className="truncate">
      {isRouteConfigured
        ? route!.name
        : t("providerForm.modelRouteConfigure", {
            defaultValue: "配置路由",
          })}
    </span>
  </Button>
</div>
```

> 已配置态：按钮带蓝色光晕（`border-blue-500/60 shadow-[0_0_10px_rgba(59,130,246,0.35)]`）+ 蓝色圆点 + 显示厂商名。

---

## 4. 路由编辑弹窗（新组件 `src/components/providers/forms/ModelFamilyRouteEditor.tsx`）

独立 `Dialog` 模态弹窗（居中），非 `FullScreenPanel`。**接收 `role` prop**（点击哪一行传入哪一角色），标题动态生成。

### 4.1 组件签名

```ts
interface ModelFamilyRouteEditorProps {
  open: boolean;
  role: ModelRole;
  initialRoute: ModelFamilyRoute | null;
  onClose: () => void;
  onSave: (role: ModelRole, route: ModelFamilyRoute) => void;
  onClear: (role: ModelRole) => void;
}
```

### 4.2 内部状态

```ts
const [name, setName] = useState("");
const [baseUrl, setBaseUrl] = useState("");
const [apiKey, setApiKey] = useState("");
const [apiKeyField, setApiKeyField] = useState<ClaudeApiKeyField>("ANTHROPIC_AUTH_TOKEN");
const [apiFormat, setApiFormat] = useState<ClaudeApiFormat>("anthropic");
const [isFullUrl, setIsFullUrl] = useState(false);
// 目标模型：存纯模型名（不含 [1M] 标记）；1M 声明用独立布尔 state。
const [targetModel, setTargetModel] = useState("");
const [targetUsesOneM, setTargetUsesOneM] = useState(false);
const [fetchedModels, setFetchedModels] = useState<FetchedModel[]>([]);
const [isFetchingModels, setIsFetchingModels] = useState(false);
const [isFetchingModelsQuick, setIsFetchingModelsQuick] = useState(false);
```

### 4.3 打开时重置（关键：防复用旧值）

```ts
useEffect(() => {
  if (!open) return;
  setName(initialRoute?.name ?? "");
  setBaseUrl(initialRoute?.baseUrl ?? "");
  setApiKey(initialRoute?.apiKey ?? "");
  setApiKeyField(initialRoute?.apiKeyField ?? "ANTHROPIC_AUTH_TOKEN");
  setApiFormat(initialRoute?.apiFormat ?? "anthropic");
  setIsFullUrl(initialRoute?.isFullUrl ?? false);
  setTargetModel(stripClaudeOneMMarker(initialRoute?.model ?? ""));
  setTargetUsesOneM(hasClaudeOneMMarker(initialRoute?.model ?? ""));
  setFetchedModels([]);
  setIsFetchingModels(false);
  setIsFetchingModelsQuick(false);
}, [open, initialRoute]);
```

> `stripClaudeOneMMarker` / `hasClaudeOneMMarker` 从 `./hooks/useModelState` 导入。

### 4.4 角色标签映射

```ts
const roleLabelMap: Record<ModelRole, string> = {
  sonnet: t("providerForm.modelRoleSonnet", { defaultValue: "Sonnet" }),
  opus: t("providerForm.modelRoleOpus", { defaultValue: "Opus" }),
  fable: t("providerForm.modelRoleFable", { defaultValue: "Fable" }),
  haiku: t("providerForm.modelRoleHaiku", { defaultValue: "Haiku" }),
  subagent: t("providerForm.modelRoleSubagent", { defaultValue: "Subagent" }),
};
```

### 4.5 获取模型列表（复用既有 API）

```ts
const handleFetchModels = useCallback(async () => {
  if (!baseUrl.trim() || !apiKey.trim()) {
    showFetchModelsError(null, t, { hasApiKey: !!apiKey, hasBaseUrl: !!baseUrl });
    return;
  }
  setIsFetchingModels(true);
  try {
    const models = await fetchModelsForConfig(baseUrl, apiKey, isFullUrl, undefined, undefined);
    setFetchedModels(models);
    if (models.length === 0) toast.info(t("providerForm.fetchModelsEmpty"));
    else toast.success(t("providerForm.fetchModelsSuccess", { count: models.length }));
  } catch (err) {
    // 不打印原始错误：上游响应可能含 base_url / 响应体细节，避免泄漏到 devtools
    console.warn("[ModelFamilyRoute] Failed to fetch models");
    showFetchModelsError(err, t);
  } finally {
    setIsFetchingModels(false);
  }
}, [baseUrl, apiKey, isFullUrl, t]);
```

### 4.6 一键设置（拉取第一个模型填入目标模型）

```ts
const handleQuickSet = useCallback(async () => {
  if (!baseUrl.trim() || !apiKey.trim()) {
    toast.error(t("providerForm.modelRouteNeedConfig", {
      defaultValue: "请先填写 Base URL 与 API Key 再获取模型列表",
    }));
    return;
  }
  setIsFetchingModelsQuick(true);
  try {
    const models = await fetchModelsForConfig(baseUrl, apiKey, isFullUrl, undefined, undefined);
    const firstModelId = models[0]?.id ?? "";
    setTargetModel(firstModelId);
    if (firstModelId) toast.success(t("providerForm.modelRouteQuickSetSuccess", {
      defaultValue: "已用第一个模型填充目标模型",
    }));
    else toast.info(t("providerForm.fetchModelsEmpty"));
  } catch (err) {
    console.warn("[ModelFamilyRoute] Quick set failed");
    showFetchModelsError(err, t);
  } finally {
    setIsFetchingModelsQuick(false);
  }
}, [baseUrl, apiKey, isFullUrl, t]);
```

### 4.7 保存（构造 `ModelFamilyRoute`）

```ts
const handleSave = () => {
  const trimmedName = name.trim();
  const trimmedBaseUrl = baseUrl.trim();
  if (!trimmedName || !trimmedBaseUrl) {
    toast.error(t("providerForm.modelRouteNeedNameAndUrl", {
      defaultValue: "请填写供应商名称与 Base URL",
    }));
    return;
  }

  // 空值不写该字段：apiKey/apiKeyField/apiFormat/model/isFullUrl 为空或默认时不落盘
  const trimmedModel = targetModel.trim();
  const route: ModelFamilyRoute = {
    name: trimmedName,
    baseUrl: trimmedBaseUrl,
    ...(apiKey.trim() ? { apiKey: apiKey.trim() } : {}),
    ...(apiKeyField !== "ANTHROPIC_AUTH_TOKEN" ? { apiKeyField } : {}),
    ...(apiFormat !== "anthropic" ? { apiFormat } : {}),
    ...(trimmedModel
      ? { model: targetUsesOneM ? `${trimmedModel}[1M]` : trimmedModel }
      : {}),
    ...(isFullUrl ? { isFullUrl: true } : {}),
  };

  onSave(role, route);
  toast.success(t("providerForm.modelRouteSaveSuccess", { defaultValue: "路由已保存" }));
};
```

### 4.8 清除路由

```ts
const handleClear = () => {
  if (window.confirm(t("providerForm.modelRouteClearConfirm", {
    defaultValue: "确定清除该角色的路由配置？",
  }))) {
    onClear(role);
  }
};
```

### 4.9 JSX 布局

| 区域 | 内容 |
|---|---|
| 头部 | `DialogTitle` = 「配置 {{role}} 路由」；`DialogClose`（X 图标，`aria-label` = 关闭） |
| 供应商名称 | `FormLabel` + `<Input id="modelRouteName">` |
| Base URL | 复用 `<EndpointField>`（`showManageButton={false}`、`showFullUrlToggle`、`isFullUrl`/`onFullUrlChange`） |
| API Key | 复用 `<ApiKeySection>`（`shouldShowLink={false}`）+ 下方提示"留空则继承主供应商 Key" |
| 认证字段 | `<Select>`（`ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_API_KEY`） |
| API 格式 | `<Select>`（anthropic / openai_chat / openai_responses / gemini_native） |
| 模型映射（简化） | 提示文字 + 「一键设置」「获取模型列表」按钮 + 单行「`{role}`｜目标模型输入(`<ModelInputWithFetch>`)｜声明支持 1M 复选框」 |
| 底部 | 左：已有路由时显示「清除路由」（ghost/destructive）；右：「保存」（含 Save 图标） |

**注意**：
- `DialogContent` 用 `zIndex="top"`，`className="max-w-xl"`。
- 两个 `SelectContent` 需显式 `z-[120]` 浮于弹窗之上（否则被 `z-[110]` 弹窗遮住）。
- 全部按钮 `type="button"`（避免误触发表单提交）。
- `EndpointField` / `ApiKeySection` / `ModelInputWithFetch` 从 `./shared` 导入（这些是共享叶子组件，与内置供应商编辑页一致）。

---

## 5. 状态管理与提交（`src/components/providers/forms/ProviderForm.tsx`）

### 5.1 import

```ts
import { ModelFamilyRouteEditor } from "./ModelFamilyRouteEditor";
import type { ModelRole, ModelFamilyRoute, ModelFamilyRouteMap } from "@/types";
```

### 5.2 重置 effect（已有 effect 内追加）

在既有 `useEffect`（依赖 `[appId, initialData, supportsFullUrl]`）末尾追加：

```ts
setModelFamilyRoutes(initialData?.meta?.modelFamilyRoutes ?? {});
setEditingRoute(null);
```

### 5.3 state 声明（在 `useCodexConfigState` 相关 state 附近）

```ts
// 模型家族路由：Claude Code 各角色 → 独立上游供应商（仅存于 meta，不写 live）
const [modelFamilyRoutes, setModelFamilyRoutes] = useState<ModelFamilyRouteMap>(
  () => initialData?.meta?.modelFamilyRoutes ?? {},
);

const [editingRoute, setEditingRoute] = useState<{
  role: ModelRole;
  route: ModelFamilyRoute | null;
} | null>(null);
```

### 5.4 `performSubmit` 中合并 meta

在 `const baseMeta = ...`（约第 1556 行）之后插入**清理逻辑**：

```ts
// 模型家族路由：只保留 name/baseUrl 均非空的条目，避免空路由落入 meta；
// 可选字段空字符串/默认值不落盘（与编辑器 handleSave 口径一致，防止 DB 残留空值）。
const cleanModelFamilyRoutes: ModelFamilyRouteMap = {};
for (const [role, route] of Object.entries(modelFamilyRoutes) as Array<
  [ModelRole, ModelFamilyRoute]
>) {
  if (route && route.name.trim() && route.baseUrl.trim()) {
    const trimmed: ModelFamilyRoute = {
      name: route.name.trim(),
      baseUrl: route.baseUrl.trim(),
    };
    if (route.apiKey?.trim()) trimmed.apiKey = route.apiKey.trim();
    if (route.apiKeyField && route.apiKeyField !== "ANTHROPIC_AUTH_TOKEN") {
      trimmed.apiKeyField = route.apiKeyField;
    }
    if (route.apiFormat && route.apiFormat !== "anthropic") {
      trimmed.apiFormat = route.apiFormat;
    }
    if (route.model?.trim()) trimmed.model = route.model.trim();
    if (route.isFullUrl) trimmed.isFullUrl = true;
    cleanModelFamilyRoutes[role] = trimmed;
  }
}
const hasModelFamilyRoutes = Object.keys(cleanModelFamilyRoutes).length > 0;

// 清空路由时需把 baseMeta 里遗留的旧路由一并移除，否则会残留空 map
if (baseMeta && "modelFamilyRoutes" in baseMeta) {
  delete baseMeta.modelFamilyRoutes;
}
```

在 `nextMeta` 对象字面量的 `isFullUrl` 之后追加：

```ts
  ...(hasModelFamilyRoutes
    ? { modelFamilyRoutes: cleanModelFamilyRoutes }
    : {}),
```

### 5.5 传给 `ClaudeFormFields`

在 `<ClaudeFormFields ... />` 的 props 中追加：

```tsx
modelFamilyRoutes={modelFamilyRoutes}
onConfigureRoute={(role, route) => setEditingRoute({ role, route })}
isProxyTakeover={isProxyTakeover}
```

### 5.6 渲染编辑器（在 `</form>` 之后、`<Form>` 之内）

```tsx
<ModelFamilyRouteEditor
  open={editingRoute !== null}
  role={editingRoute?.role ?? "sonnet"}
  initialRoute={editingRoute?.route ?? null}
  onClose={() => setEditingRoute(null)}
  onSave={(role, route) => {
    setModelFamilyRoutes((prev) => ({ ...prev, [role]: route }));
    setEditingRoute(null);
  }}
  onClear={(role) => {
    setModelFamilyRoutes((prev) => {
      const next = { ...prev };
      delete next[role];
      return next;
    });
    setEditingRoute(null);
  }}
/>
```

> 编辑器渲染在 `<Form>` 内以复用 `FormLabel`/`EndpointField` 上下文；它本身是 `Dialog`，DOM 位置不影响布局。

---

## 6. i18n 文案（4 个语言文件均需添加）

`src/i18n/locales/{en,ja,zh-TW,zh}.json` 的 `providerForm` 对象下新增（17 条）：

```json
"modelRouteConfigure": "配置路由",
"modelRouteHeader": "配置路由",
"modelRouteHint": "模型家族路由需在 Claude 本地路由开启时生效",
"modelRouteHintActive": "模型家族路由已生效（Claude 本地路由接管中）",
"modelRouteEditorTitle": "配置 {{role}} 路由",
"modelRouteTargetModelLabel": "目标模型",
"modelRouteTargetModelHint": "当前路由只能为一个 Claude Code 模型角色选择一个目标模型。",
"modelRouteClear": "清除路由",
"modelRouteClearConfirm": "确定清除该角色的路由配置？",
"modelRouteSaveSuccess": "路由已保存",
"modelRouteApiKeyInheritHint": "留空则继承主供应商 Key",
"modelRouteQuickSetSuccess": "已用第一个模型填充目标模型",
"modelRouteNeedConfig": "请先填写 Base URL 与 API Key 再获取模型列表",
"modelRouteNeedNameAndUrl": "请填写供应商名称与 Base URL",
"modelRouteVendorName": "供应商名称",
"modelRouteVendorNamePlaceholder": "例如 DeepSeek 官方",
"modelRouteApiKeyPlaceholder": "选填，留空则继承主供应商 Key"
```

> `en.json` / `ja.json` / `zh-TW.json` 用对应翻译；key 名保持一致。

---

## 7. 测试

- `tests/components/ModelFamilyRouteEditor.test.tsx`（新增，10 个用例）：新建/编辑路由、清除路由回调、保存回调载荷、一键设置填充、1M 复选框与 `[1M]` 后缀联动、baseUrl/apiKey 缺失时的提示。
- `tests/components/ClaudeFormFields.test.tsx`（更新，+8 用例）：传入 `modelFamilyRoutes` 时显示供应商名、路由按钮蓝色光晕、未配置时正常渲染输入框。

---

## 8. 依赖的既有共享组件（无需新建，直接复用）

| 组件 | 路径 |
|---|---|
| `ApiKeySection` | `src/components/providers/forms/shared/ApiKeySection.tsx` |
| `EndpointField` | `src/components/providers/forms/shared/EndpointField.tsx` |
| `ModelInputWithFetch` | `src/components/providers/forms/shared/ModelInputWithFetch.tsx` |
| `hasClaudeOneMMarker` / `stripClaudeOneMMarker` | `src/components/providers/forms/hooks/useModelState.ts` |
| `fetchModelsForConfig` / `showFetchModelsError` | `src/lib/api/model-fetch.ts` |
| `Dialog` / `DialogContent` / `DialogClose` / `DialogTitle` | `src/components/ui/dialog.tsx` |
| `Button` / `Input` / `Select` 系列 / `Checkbox` / `FormLabel` | `src/components/ui/*` |

---

## 9. 后端配套（必须同步实现，否则路由不生效）

前端只负责"配置存储"。**实际分流在后端 Rust 代理**：

- `src-tauri/src/provider.rs`：`ProviderMeta` 新增 `model_family_routes` 字段 + `ModelFamilyRoute` 结构体（serde 名与 TS 严格一致）。
- `src-tauri/src/proxy/model_mapper.rs`：`classify_model_role` / `resolve_model_family_route` / `derive_routed_provider`。
- `src-tauri/src/proxy/forwarder.rs`：`forward` 开头（`extract_base_url` 之前）插入路由解析，命中则用派生 Provider 继续。

完整方案见 `docs/design/model-family-routing.md` §3-§4。

> **版本提醒**：前端提交的 `modelFamilyRoutes` 在旧后端（无该字段）会被 serde 静默丢弃 → 表现为"保存后再进入配置消失"。必须使用含后端实现的新版后端二进制。
