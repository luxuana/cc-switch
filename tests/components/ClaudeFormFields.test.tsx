import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ComponentProps, PropsWithChildren } from "react";
import { useForm } from "react-hook-form";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ClaudeFormFields } from "@/components/providers/forms/ClaudeFormFields";
import { Form } from "@/components/ui/form";

const copilotApiMock = vi.hoisted(() => ({
  copilotGetModels: vi.fn(),
  copilotGetModelsForAccount: vi.fn(),
}));

const modelFetchApiMock = vi.hoisted(() => ({
  fetchCodexOauthModels: vi.fn(),
  fetchModelsForConfig: vi.fn(),
  showFetchModelsError: vi.fn(),
}));

vi.mock("@/lib/api/copilot", () => ({
  copilotGetModels: copilotApiMock.copilotGetModels,
  copilotGetModelsForAccount: copilotApiMock.copilotGetModelsForAccount,
}));

vi.mock("@/lib/api/model-fetch", () => ({
  fetchCodexOauthModels: modelFetchApiMock.fetchCodexOauthModels,
  fetchModelsForConfig: modelFetchApiMock.fetchModelsForConfig,
  showFetchModelsError: modelFetchApiMock.showFetchModelsError,
}));

vi.mock("@/components/providers/forms/CopilotAuthSection", () => ({
  CopilotAuthSection: () => <div data-testid="copilot-auth-section" />,
}));

vi.mock("@/components/providers/forms/CodexOAuthSection", () => ({
  CodexOAuthSection: () => <div data-testid="codex-oauth-section" />,
}));

type ClaudeFormFieldsProps = ComponentProps<typeof ClaudeFormFields>;

const FormShell = ({ children }: PropsWithChildren) => {
  const form = useForm();

  return <Form {...form}>{children}</Form>;
};

const renderCopilotForm = (overrides: Partial<ClaudeFormFieldsProps> = {}) => {
  const props: ClaudeFormFieldsProps = {
    shouldShowApiKey: false,
    apiKey: "",
    onApiKeyChange: vi.fn(),
    category: "official",
    shouldShowApiKeyLink: false,
    websiteUrl: "",
    isCopilotPreset: true,
    usesOAuth: true,
    isCopilotAuthenticated: true,
    selectedGitHubAccountId: "gh-1",
    onGitHubAccountSelect: vi.fn(),
    isCodexOauthPreset: false,
    isCodexOauthAuthenticated: false,
    selectedCodexAccountId: null,
    onCodexAccountSelect: vi.fn(),
    codexFastMode: false,
    onCodexFastModeChange: vi.fn(),
    templateValueEntries: [],
    templateValues: {},
    templatePresetName: "",
    onTemplateValueChange: vi.fn(),
    shouldShowSpeedTest: false,
    baseUrl: "",
    onBaseUrlChange: vi.fn(),
    isEndpointModalOpen: false,
    onEndpointModalToggle: vi.fn(),
    onCustomEndpointsChange: vi.fn(),
    autoSelect: false,
    onAutoSelectChange: vi.fn(),
    showEndpointTools: true,
    shouldShowModelSelector: true,
    claudeModel: "",
    defaultHaikuModel: "",
    defaultHaikuModelName: "",
    defaultSonnetModel: "claude-sonnet",
    defaultSonnetModelName: "Claude Sonnet",
    defaultOpusModel: "",
    defaultOpusModelName: "",
    defaultFableModel: "",
    defaultFableModelName: "",
    subagentModel: "",
    onModelChange: vi.fn(),
    speedTestEndpoints: [],
    apiFormat: "anthropic",
    onApiFormatChange: vi.fn(),
    apiKeyField: "ANTHROPIC_AUTH_TOKEN",
    onApiKeyFieldChange: vi.fn(),
    isFullUrl: false,
    onFullUrlChange: vi.fn(),
    customUserAgent: "",
    onCustomUserAgentChange: vi.fn(),
    localProxyHeadersOverride: "",
    onLocalProxyHeadersOverrideChange: vi.fn(),
    localProxyBodyOverride: "",
    onLocalProxyBodyOverrideChange: vi.fn(),
    ...overrides,
  };

  return render(
    <FormShell>
      <ClaudeFormFields {...props} />
    </FormShell>,
  );
};

const renderCodexOauthForm = (overrides: Partial<ClaudeFormFieldsProps> = {}) =>
  renderCopilotForm({
    isCopilotPreset: false,
    isCopilotAuthenticated: false,
    selectedGitHubAccountId: null,
    isCodexOauthPreset: true,
    isCodexOauthAuthenticated: true,
    selectedCodexAccountId: "chatgpt-1",
    ...overrides,
  });

// 普通 Claude 供应商表单（非 Copilot / 非 Codex OAuth），用于模型家族路由测试
const renderPlainForm = (overrides: Partial<ClaudeFormFieldsProps> = {}) =>
  renderCopilotForm({
    isCopilotPreset: false,
    isCopilotAuthenticated: false,
    selectedGitHubAccountId: null,
    usesOAuth: false,
    ...overrides,
  });

const SONNET_ROUTE = {
  name: "DeepSeek 官方",
  baseUrl: "https://api.deepseek.com/anthropic",
};

// Sonnet 行的角色标签单元格（每个角色 label 只出现一次）
function sonnetRow(): HTMLElement {
  const roleLabel = screen.getByText("Sonnet");
  const grid = roleLabel.closest(".grid");
  if (!grid) throw new Error("sonnet row grid not found");
  return grid as HTMLElement;
}

describe("ClaudeFormFields", () => {
  beforeEach(() => {
    copilotApiMock.copilotGetModels.mockResolvedValue([]);
    copilotApiMock.copilotGetModelsForAccount.mockResolvedValue([]);
    modelFetchApiMock.fetchCodexOauthModels.mockResolvedValue([]);
    modelFetchApiMock.fetchModelsForConfig.mockResolvedValue([]);
  });

  it("不会在 Copilot 表单打开时自动获取模型列表", () => {
    renderCopilotForm();

    expect(copilotApiMock.copilotGetModels).not.toHaveBeenCalled();
    expect(copilotApiMock.copilotGetModelsForAccount).not.toHaveBeenCalled();
  });

  it("点击获取模型列表后才请求当前 Copilot 账号的模型", async () => {
    renderCopilotForm();

    fireEvent.click(
      screen.getByRole("button", {
        name: "providerForm.fetchModels",
      }),
    );

    await waitFor(() => {
      expect(copilotApiMock.copilotGetModelsForAccount).toHaveBeenCalledWith(
        "gh-1",
      );
    });
    expect(copilotApiMock.copilotGetModels).not.toHaveBeenCalled();
  });

  it("不会在 Codex OAuth 表单打开时自动获取模型列表", () => {
    renderCodexOauthForm();

    expect(modelFetchApiMock.fetchCodexOauthModels).not.toHaveBeenCalled();
  });

  it("点击获取模型列表后才请求当前 Codex OAuth 账号的模型", async () => {
    renderCodexOauthForm();

    fireEvent.click(
      screen.getByRole("button", {
        name: "providerForm.fetchModels",
      }),
    );

    await waitFor(() => {
      expect(modelFetchApiMock.fetchCodexOauthModels).toHaveBeenCalledWith(
        "chatgpt-1",
      );
    });
  });

  it("一键设置会同时写入 Subagent 模型", () => {
    const onModelChange = vi.fn();
    renderCopilotForm({
      claudeModel: "shared-model[1M]",
      defaultSonnetModel: "",
      defaultSonnetModelName: "",
      onModelChange,
    });

    fireEvent.click(
      screen.getByRole("button", {
        name: "一键设置",
      }),
    );

    expect(onModelChange).toHaveBeenCalledWith(
      "CLAUDE_CODE_SUBAGENT_MODEL",
      "shared-model[1M]",
    );
  });

  describe("模型家族路由", () => {
    it("未配置路由时渲染普通模型输入框", () => {
      renderPlainForm();

      // Sonnet 行仍是输入框
      expect(
        document.getElementById("claudeDefaultSonnetModel"),
      ).toBeTruthy();
      // 未配置的行显示「配置路由」按钮
      const routeButton = within(sonnetRow()).getByRole("button", {
        name: "配置路由",
      });
      expect(routeButton).toBeTruthy();
      expect(routeButton.className).not.toContain(
        "shadow-[0_0_10px_rgba(59,130,246,0.35)]",
      );
    });

    it("已配置路由时「实际请求模型」列显示供应商名", () => {
      renderPlainForm({
        modelFamilyRoutes: { sonnet: SONNET_ROUTE },
      });

      // Sonnet 行的「实际请求模型」列显示供应商名称，而非模型输入框
      expect(
        document.getElementById("claudeDefaultSonnetModel"),
      ).toBeNull();
      expect(within(sonnetRow()).getAllByText("DeepSeek 官方").length).toBe(
        2, // 供应商名徽标 + 配置路由按钮文字
      );
    });

    it("已配置路由时供应商名带蓝色边框", () => {
      renderPlainForm({
        modelFamilyRoutes: { sonnet: SONNET_ROUTE },
      });

      // 供应商名徽标：带蓝色边框的 div（区别于按钮文字）
      const badge = within(sonnetRow())
        .getAllByText("DeepSeek 官方")
        .find((el) => el.className.includes("border-blue-500/40"));
      expect(badge).toBeTruthy();
      expect(badge!.className).toContain("border-blue-500/40");
    });

    it("已配置路由的按钮显示厂商名并带蓝色光晕", () => {
      renderPlainForm({
        modelFamilyRoutes: { sonnet: SONNET_ROUTE },
      });

      const routeButton = within(sonnetRow()).getByRole("button", {
        name: "DeepSeek 官方",
      });
      expect(routeButton.className).toContain(
        "border-blue-500/60 shadow-[0_0_10px_rgba(59,130,246,0.35)]",
      );
      // 蓝色圆点
      expect(routeButton.querySelector(".bg-blue-500")).toBeTruthy();
    });

    it("点击已配置路由按钮触发 onConfigureRoute 并传入当前路由", () => {
      const onConfigureRoute = vi.fn();
      renderPlainForm({
        modelFamilyRoutes: { sonnet: SONNET_ROUTE },
        onConfigureRoute,
      });

      fireEvent.click(
        within(sonnetRow()).getByRole("button", { name: "DeepSeek 官方" }),
      );
      expect(onConfigureRoute).toHaveBeenCalledWith("sonnet", SONNET_ROUTE);
    });

    it("点击未配置路由按钮触发 onConfigureRoute 并传入 null", () => {
      const onConfigureRoute = vi.fn();
      renderPlainForm({ onConfigureRoute });

      fireEvent.click(
        within(sonnetRow()).getByRole("button", { name: "配置路由" }),
      );
      expect(onConfigureRoute).toHaveBeenCalledWith("sonnet", null);
    });

    it("模型映射区显示「未开启路由」提示文案", () => {
      renderPlainForm({ isProxyTakeover: false });
      expect(
        screen.getByText("模型家族路由需在 Claude 本地路由开启时生效"),
      ).toBeTruthy();
    });

    it("代理接管时显示「路由已生效」提示文案", () => {
      renderPlainForm({ isProxyTakeover: true });
      expect(
        screen.getByText("模型家族路由已生效（Claude 本地路由接管中）"),
      ).toBeTruthy();
    });
  });
});
