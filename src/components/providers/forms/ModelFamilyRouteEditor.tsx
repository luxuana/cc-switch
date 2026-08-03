import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogClose,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { FormLabel } from "@/components/ui/form";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Save, Trash2, Download, Loader2, Wand2, X } from "lucide-react";
import { ApiKeySection, EndpointField, ModelInputWithFetch } from "./shared";
import {
  fetchModelsForConfig,
  showFetchModelsError,
  type FetchedModel,
} from "@/lib/api/model-fetch";
import {
  hasClaudeOneMMarker,
  stripClaudeOneMMarker,
} from "./hooks/useModelState";
import type {
  ClaudeApiFormat,
  ClaudeApiKeyField,
  ModelFamilyRoute,
  ModelRole,
} from "@/types";

interface ModelFamilyRouteEditorProps {
  open: boolean;
  role: ModelRole;
  initialRoute: ModelFamilyRoute | null;
  /** 主 Provider 的 meta.isFullUrl 继承值：路由未显式设置 isFullUrl 时，
   *  后端 derive 会继承此值，弹窗打开时回退读它，避免 UI 显示与后端继承不一致。 */
  defaultIsFullUrl?: boolean;
  onClose: () => void;
  onSave: (role: ModelRole, route: ModelFamilyRoute) => void;
  onClear: (role: ModelRole) => void;
}

export function ModelFamilyRouteEditor({
  open,
  role,
  initialRoute,
  defaultIsFullUrl = false,
  onClose,
  onSave,
  onClear,
}: ModelFamilyRouteEditorProps) {
  const { t } = useTranslation();

  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [apiKeyField, setApiKeyField] =
    useState<ClaudeApiKeyField>("ANTHROPIC_AUTH_TOKEN");
  const [apiFormat, setApiFormat] = useState<ClaudeApiFormat>("anthropic");
  const [isFullUrl, setIsFullUrl] = useState(false);
  // 目标模型：存纯模型名（不含 [1M] 标记）；1M 声明用独立布尔 state。
  // 双重语义：上游收到它（代理改写 body.model）；接管时 CC 端 _MODEL 也写它
  //（替代固定接管别名），Claude Code 显示它并按它做能力判定（xhigh）。
  const [targetModel, setTargetModel] = useState("");
  const [targetUsesOneM, setTargetUsesOneM] = useState(false);
  const [fetchedModels, setFetchedModels] = useState<FetchedModel[]>([]);
  const [isFetchingModels, setIsFetchingModels] = useState(false);
  const [isFetchingModelsQuick, setIsFetchingModelsQuick] = useState(false);

  // 打开时重置（防复用旧值）
  useEffect(() => {
    if (!open) return;
    setName(initialRoute?.name ?? "");
    setBaseUrl(initialRoute?.baseUrl ?? "");
    setApiKey(initialRoute?.apiKey ?? "");
    setApiKeyField(initialRoute?.apiKeyField ?? "ANTHROPIC_AUTH_TOKEN");
    setApiFormat(initialRoute?.apiFormat ?? "anthropic");
    setIsFullUrl(initialRoute?.isFullUrl ?? defaultIsFullUrl);
    setTargetModel(stripClaudeOneMMarker(initialRoute?.model ?? ""));
    setTargetUsesOneM(hasClaudeOneMMarker(initialRoute?.model ?? ""));
    setFetchedModels([]);
    setIsFetchingModels(false);
    setIsFetchingModelsQuick(false);
  }, [open, initialRoute]);

  const roleLabelMap: Record<ModelRole, string> = {
    sonnet: t("providerForm.modelRoleSonnet", { defaultValue: "Sonnet" }),
    opus: t("providerForm.modelRoleOpus", { defaultValue: "Opus" }),
    fable: t("providerForm.modelRoleFable", { defaultValue: "Fable" }),
    haiku: t("providerForm.modelRoleHaiku", { defaultValue: "Haiku" }),
    subagent: t("providerForm.modelRoleSubagent", {
      defaultValue: "Subagent",
    }),
  };

  const handleFetchModels = useCallback(async () => {
    if (!baseUrl.trim() || !apiKey.trim()) {
      showFetchModelsError(null, t, {
        hasApiKey: !!apiKey,
        hasBaseUrl: !!baseUrl,
      });
      return;
    }
    setIsFetchingModels(true);
    try {
      const models = await fetchModelsForConfig(
        baseUrl,
        apiKey,
        isFullUrl,
        undefined,
        undefined,
      );
      setFetchedModels(models);
      if (models.length === 0) toast.info(t("providerForm.fetchModelsEmpty"));
      else
        toast.success(
          t("providerForm.fetchModelsSuccess", { count: models.length }),
        );
    } catch (err) {
      // 不打印原始错误：上游响应可能含 base_url / 响应体细节，避免泄漏到 devtools
      console.warn("[ModelFamilyRoute] Failed to fetch models");
      showFetchModelsError(err, t);
    } finally {
      setIsFetchingModels(false);
    }
  }, [baseUrl, apiKey, isFullUrl, t]);

  const handleQuickSet = useCallback(async () => {
    if (!baseUrl.trim() || !apiKey.trim()) {
      toast.error(
        t("providerForm.modelRouteNeedConfig", {
          defaultValue: "请先填写 Base URL 与 API Key 再获取模型列表",
        }),
      );
      return;
    }
    setIsFetchingModelsQuick(true);
    try {
      const models = await fetchModelsForConfig(
        baseUrl,
        apiKey,
        isFullUrl,
        undefined,
        undefined,
      );
      const firstModelId = models[0]?.id ?? "";
      setTargetModel(firstModelId);
      if (firstModelId)
        toast.success(
          t("providerForm.modelRouteQuickSetSuccess", {
            defaultValue: "已用第一个模型填充目标模型",
          }),
        );
      else toast.info(t("providerForm.fetchModelsEmpty"));
    } catch (err) {
      console.warn("[ModelFamilyRoute] Quick set failed");
      showFetchModelsError(err, t);
    } finally {
      setIsFetchingModelsQuick(false);
    }
  }, [baseUrl, apiKey, isFullUrl, t]);

  const handleSave = () => {
    const trimmedName = name.trim();
    const trimmedBaseUrl = baseUrl.trim();
    if (!trimmedName || !trimmedBaseUrl) {
      toast.error(
        t("providerForm.modelRouteNeedNameAndUrl", {
          defaultValue: "请填写供应商名称与 Base URL",
        }),
      );
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
    toast.success(
      t("providerForm.modelRouteSaveSuccess", { defaultValue: "路由已保存" }),
    );
  };

  const handleClear = () => {
    if (
      window.confirm(
        t("providerForm.modelRouteClearConfirm", {
          defaultValue: "确定清除该角色的路由配置？",
        }),
      )
    ) {
      onClear(role);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent zIndex="top" className="max-w-xl">
        <DialogHeader>
          <DialogTitle>
            {t("providerForm.modelRouteEditorTitle", {
              role: roleLabelMap[role],
              defaultValue: "配置 {{role}} 路由",
            })}
          </DialogTitle>
          <DialogClose asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="absolute right-4 top-4 h-8 w-8"
              aria-label={t("common.close", { defaultValue: "关闭" })}
            >
              <X className="h-4 w-4" />
            </Button>
          </DialogClose>
        </DialogHeader>

        <div className="space-y-4 overflow-y-auto px-6 py-4">
          {/* 供应商名称 */}
          <div className="space-y-2">
            <FormLabel htmlFor="modelRouteName">
              {t("providerForm.modelRouteVendorName", {
                defaultValue: "供应商名称",
              })}
            </FormLabel>
            <Input
              id="modelRouteName"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("providerForm.modelRouteVendorNamePlaceholder", {
                defaultValue: "例如 DeepSeek 官方",
              })}
              autoComplete="off"
            />
          </div>

          {/* Base URL */}
          <EndpointField
            id="modelRouteBaseUrl"
            label={t("providerForm.apiEndpoint")}
            value={baseUrl}
            onChange={setBaseUrl}
            placeholder={t("providerForm.apiEndpointPlaceholder")}
            showManageButton={false}
            showFullUrlToggle
            isFullUrl={isFullUrl}
            onFullUrlChange={setIsFullUrl}
          />

          {/* API Key */}
          <div className="space-y-1">
            <ApiKeySection
              value={apiKey}
              onChange={setApiKey}
              shouldShowLink={false}
              websiteUrl=""
              placeholder={{
                official: t("providerForm.modelRouteApiKeyPlaceholder", {
                  defaultValue: "选填，留空则继承主供应商 Key",
                }),
                thirdParty: t("providerForm.modelRouteApiKeyPlaceholder", {
                  defaultValue: "选填，留空则继承主供应商 Key",
                }),
              }}
            />
            <p className="text-xs text-muted-foreground">
              {t("providerForm.modelRouteApiKeyInheritHint", {
                defaultValue: "留空则继承主供应商 Key",
              })}
            </p>
          </div>

          {/* 认证字段 */}
          <div className="space-y-2">
            <FormLabel>
              {t("providerForm.authField", { defaultValue: "认证字段" })}
            </FormLabel>
            <Select
              value={apiKeyField}
              onValueChange={(v) => setApiKeyField(v as ClaudeApiKeyField)}
            >
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent className="z-[120]">
                <SelectItem value="ANTHROPIC_AUTH_TOKEN">
                  {t("providerForm.authFieldAuthToken", {
                    defaultValue: "ANTHROPIC_AUTH_TOKEN（默认）",
                  })}
                </SelectItem>
                <SelectItem value="ANTHROPIC_API_KEY">
                  {t("providerForm.authFieldApiKey", {
                    defaultValue: "ANTHROPIC_API_KEY",
                  })}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* API 格式 */}
          <div className="space-y-2">
            <FormLabel>
              {t("providerForm.apiFormat", { defaultValue: "API 格式" })}
            </FormLabel>
            <Select
              value={apiFormat}
              onValueChange={(v) => setApiFormat(v as ClaudeApiFormat)}
            >
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent className="z-[120]">
                <SelectItem value="anthropic">
                  {t("providerForm.apiFormatAnthropic", {
                    defaultValue: "Anthropic Messages (原生)",
                  })}
                </SelectItem>
                <SelectItem value="openai_chat">
                  {t("providerForm.apiFormatOpenAIChat", {
                    defaultValue: "OpenAI Chat Completions (需转换)",
                  })}
                </SelectItem>
                <SelectItem value="openai_responses">
                  {t("providerForm.apiFormatOpenAIResponses", {
                    defaultValue: "OpenAI Responses API (需转换)",
                  })}
                </SelectItem>
                <SelectItem value="gemini_native">
                  {t("providerForm.apiFormatGeminiNative", {
                    defaultValue: "Gemini Native generateContent (需转换)",
                  })}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* 模型映射（简化） */}
          <div className="space-y-2 border-t pt-3">
            <div className="flex items-center justify-between">
              <FormLabel>{t("providerForm.modelMappingLabel")}</FormLabel>
              <div className="flex gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={handleQuickSet}
                  disabled={isFetchingModelsQuick || isFetchingModels}
                  className="h-7 gap-1"
                >
                  {isFetchingModelsQuick ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Wand2 className="h-3.5 w-3.5" />
                  )}
                  {t("providerForm.quickSetModels", {
                    defaultValue: "一键设置",
                  })}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={handleFetchModels}
                  disabled={isFetchingModels || isFetchingModelsQuick}
                  className="h-7 gap-1"
                >
                  {isFetchingModels ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Download className="h-3.5 w-3.5" />
                  )}
                  {t("providerForm.fetchModels")}
                </Button>
              </div>
            </div>
            <p className="text-xs text-muted-foreground">
              {t("providerForm.modelRouteTargetModelHint", {
                defaultValue:
                  "目标模型同时决定：① 发给上游的实际模型；② Claude Code 端显示的模型名与能力判定（如 xhigh 思考支持）。用户填什么，客户端就按什么判定。",
              })}
            </p>

            <div className="grid grid-cols-1 items-center gap-2 md:grid-cols-[96px_1fr_minmax(0,104px)]">
              <div className="flex h-9 items-center rounded-md border border-input bg-muted px-3 text-sm font-medium text-muted-foreground">
                {roleLabelMap[role]}
              </div>
              <ModelInputWithFetch
                id="modelRouteTargetModel"
                value={targetModel}
                onChange={setTargetModel}
                placeholder={t("providerForm.modelPlaceholder", {
                  defaultValue: "",
                })}
                fetchedModels={fetchedModels}
                isLoading={isFetchingModels}
              />
              <label className="flex h-9 items-center gap-2 text-sm text-muted-foreground">
                <Checkbox
                  checked={targetUsesOneM}
                  onCheckedChange={(checked) =>
                    setTargetUsesOneM(checked === true)
                  }
                />
                {t("providerForm.modelOneMLabel", { defaultValue: "1M" })}
              </label>
            </div>
            <p className="text-xs text-muted-foreground">
              {t("providerForm.modelRouteModelNameHint", {
                defaultValue:
                  "留空则透传请求模型（CC 端回退固定接管别名，如 claude-sonnet-4-6，可能不支持 xhigh）。建议填写 Claude Code 认识的新模型 ID（如 claude-sonnet-5 / claude-opus-4-8）以启用 xhigh 思考。",
              })}
            </p>
          </div>
        </div>

        <DialogFooter>
          {initialRoute ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={handleClear}
              className="mr-auto gap-1 text-destructive hover:text-destructive"
            >
              <Trash2 className="h-3.5 w-3.5" />
              {t("providerForm.modelRouteClear", {
                defaultValue: "清除路由",
              })}
            </Button>
          ) : (
            <span />
          )}
          <div className="flex gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={onClose}
            >
              {t("common.cancel", { defaultValue: "取消" })}
            </Button>
            <Button type="button" onClick={handleSave} className="gap-1">
              <Save className="h-3.5 w-3.5" />
              {t("common.save", { defaultValue: "保存" })}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
