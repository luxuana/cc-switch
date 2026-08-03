import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { PropsWithChildren } from "react";
import { useForm } from "react-hook-form";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ModelFamilyRouteEditor } from "@/components/providers/forms/ModelFamilyRouteEditor";
import { Form } from "@/components/ui/form";
import type { ModelFamilyRoute, ModelRole } from "@/types";

const modelFetchMock = vi.hoisted(() => ({
  fetchModelsForConfig: vi.fn(),
  showFetchModelsError: vi.fn(),
}));

vi.mock("@/lib/api/model-fetch", () => ({
  fetchModelsForConfig: modelFetchMock.fetchModelsForConfig,
  showFetchModelsError: modelFetchMock.showFetchModelsError,
}));

const FormShell = ({ children }: PropsWithChildren) => {
  const form = useForm();
  return <Form {...form}>{children}</Form>;
};

type RenderEditorResult = {
  open: boolean;
  role: ModelRole;
  initialRoute: ModelFamilyRoute | null;
  onClose: ReturnType<typeof vi.fn>;
  onSave: ReturnType<typeof vi.fn>;
  onClear: ReturnType<typeof vi.fn>;
};

function renderEditor(
  overrides: Partial<{
    open: boolean;
    role: ModelRole;
    initialRoute: ModelFamilyRoute | null;
    onClose: () => void;
    onSave: (role: ModelRole, route: ModelFamilyRoute) => void;
    onClear: (role: ModelRole) => void;
  }> = {},
): RenderEditorResult {
  const props: RenderEditorResult = {
    open: true,
    role: "sonnet" as ModelRole,
    initialRoute: null,
    onClose: vi.fn(),
    onSave: vi.fn(),
    onClear: vi.fn(),
  };
  Object.assign(props, overrides);
  render(
    <FormShell>
      <ModelFamilyRouteEditor {...props} />
    </FormShell>,
  );
  return props;
}

// 测试 i18n resources 为空，t(key, { defaultValue }) 返回 defaultValue（zh）。
const SAVE = "保存";
const QUICK_SET = "一键设置";
const FETCH_MODELS = "providerForm.fetchModels";
const CLEAR = "清除路由";
const VENDOR_NAME_LABEL = "供应商名称";
const ENDPOINT_LABEL = "providerForm.apiEndpoint";

function targetModelInput(): HTMLInputElement {
  const el = document.getElementById("modelRouteTargetModel");
  if (!el) throw new Error("modelRouteTargetModel input not found");
  return el as HTMLInputElement;
}

function apiKeyInput(): HTMLInputElement {
  const el = document.getElementById("apiKey");
  if (!el) throw new Error("apiKey input not found");
  return el as HTMLInputElement;
}

function fillRequiredFields(overrides: {
  name?: string;
  baseUrl?: string;
  apiKey?: string;
} = {}) {
  fireEvent.change(screen.getByLabelText(VENDOR_NAME_LABEL), {
    target: { value: overrides.name ?? "DeepSeek 官方" },
  });
  fireEvent.change(screen.getByLabelText(ENDPOINT_LABEL), {
    target: {
      value: overrides.baseUrl ?? "https://api.deepseek.com/anthropic",
    },
  });
  if (overrides.apiKey !== undefined) {
    fireEvent.change(apiKeyInput(), { target: { value: overrides.apiKey } });
  }
}

describe("ModelFamilyRouteEditor", () => {
  beforeEach(() => {
    modelFetchMock.fetchModelsForConfig.mockResolvedValue([]);
    modelFetchMock.showFetchModelsError.mockImplementation(() => {});
  });

  it("打开时显示角色对应的标题", () => {
    renderEditor({ role: "fable" });
    expect(screen.getByText("配置 Fable 路由")).toBeTruthy();
  });

  it("新建路由：填写名称与 Base URL 后保存回传正确载荷", () => {
    const { onSave } = renderEditor();

    fillRequiredFields();
    fireEvent.click(screen.getByText(SAVE));

    expect(onSave).toHaveBeenCalledTimes(1);
    const [role, route] = onSave.mock.calls[0] as [ModelRole, ModelFamilyRoute];
    expect(role).toBe("sonnet");
    expect(route).toEqual({
      name: "DeepSeek 官方",
      baseUrl: "https://api.deepseek.com/anthropic",
    });
  });

  it("空字段保存：缺少名称或 Base URL 时提示且不触发 onSave", () => {
    const { onSave } = renderEditor();

    fireEvent.click(screen.getByText(SAVE));
    expect(onSave).not.toHaveBeenCalled();

    fillRequiredFields({ name: "DeepSeek 官方", baseUrl: "" });
    fireEvent.click(screen.getByText(SAVE));
    expect(onSave).not.toHaveBeenCalled();
  });

  it("编辑已有路由：回显各字段，保存时保留可选字段", () => {
    const { onSave } = renderEditor({
      initialRoute: {
        name: "DeepSeek 官方",
        baseUrl: "https://api.deepseek.com/anthropic",
        apiKey: "sk-abc",
        apiKeyField: "ANTHROPIC_API_KEY",
        apiFormat: "openai_chat",
        model: "deepseek-v4-pro[1M]",
        isFullUrl: true,
      },
    });

    // 回显
    expect(
      (screen.getByLabelText(VENDOR_NAME_LABEL) as HTMLInputElement).value,
    ).toBe("DeepSeek 官方");
    expect(
      (screen.getByLabelText(ENDPOINT_LABEL) as HTMLInputElement).value,
    ).toBe("https://api.deepseek.com/anthropic");
    // 1M 标记从目标模型中剥离，独立布尔为 true
    expect(targetModelInput().value).toBe("deepseek-v4-pro");

    fireEvent.click(screen.getByText(SAVE));

    const [role, route] = onSave.mock.calls[0] as [ModelRole, ModelFamilyRoute];
    expect(role).toBe("sonnet");
    expect(route).toMatchObject({
      name: "DeepSeek 官方",
      baseUrl: "https://api.deepseek.com/anthropic",
      apiKey: "sk-abc",
      apiKeyField: "ANTHROPIC_API_KEY",
      apiFormat: "openai_chat",
      model: "deepseek-v4-pro[1M]",
      isFullUrl: true,
    });
  });

  it("保存时不落盘空字符串与默认值", () => {
    const { onSave } = renderEditor();

    fillRequiredFields({ name: "  DeepSeek 官方  " });
    fireEvent.change(targetModelInput(), {
      target: { value: "  deepseek-v4-pro  " },
    });
    fireEvent.click(screen.getByText(SAVE));

    const [, route] = onSave.mock.calls[0] as [ModelRole, ModelFamilyRoute];
    expect(route.name).toBe("DeepSeek 官方");
    expect(route).not.toHaveProperty("apiKey");
    expect(route).not.toHaveProperty("apiKeyField");
    expect(route).not.toHaveProperty("apiFormat");
    expect(route).not.toHaveProperty("isFullUrl");
    expect(route.model).toBe("deepseek-v4-pro");
  });

  it("清除路由：已有路由时显示清除按钮，确认后调用 onClear", () => {
    const { onClear } = renderEditor({
      initialRoute: {
        name: "DeepSeek 官方",
        baseUrl: "https://api.deepseek.com/anthropic",
      },
    });

    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    fireEvent.click(screen.getByText(CLEAR));

    expect(confirmSpy).toHaveBeenCalled();
    expect(onClear).toHaveBeenCalledWith("sonnet");
    confirmSpy.mockRestore();
  });

  it("清除路由：取消确认时不调用 onClear", () => {
    const { onClear } = renderEditor({
      initialRoute: {
        name: "DeepSeek 官方",
        baseUrl: "https://api.deepseek.com/anthropic",
      },
    });

    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    fireEvent.click(screen.getByText(CLEAR));

    expect(onClear).not.toHaveBeenCalled();
    confirmSpy.mockRestore();
  });

  it("新建路由时不显示清除按钮", () => {
    renderEditor();
    expect(screen.queryByText(CLEAR)).toBeNull();
  });

  it("一键设置：拉取第一个模型填充目标模型", async () => {
    modelFetchMock.fetchModelsForConfig.mockResolvedValue([
      { id: "deepseek-chat", ownedBy: "deepseek" },
      { id: "deepseek-reasoner", ownedBy: "deepseek" },
    ]);

    renderEditor();
    fillRequiredFields({ apiKey: "sk-abc" });
    fireEvent.change(targetModelInput(), { target: { value: "" } });
    fireEvent.click(screen.getByText(QUICK_SET));

    await waitFor(() => {
      expect(targetModelInput().value).toBe("deepseek-chat");
    });
    expect(modelFetchMock.fetchModelsForConfig).toHaveBeenCalledWith(
      "https://api.deepseek.com/anthropic",
      "sk-abc",
      false,
      undefined,
      undefined,
    );
  });

  it("一键设置：缺少 Base URL 或 API Key 时提示且不请求", async () => {
    renderEditor();
    fireEvent.change(screen.getByLabelText(VENDOR_NAME_LABEL), {
      target: { value: "DeepSeek 官方" },
    });
    fireEvent.click(screen.getByText(QUICK_SET));

    expect(modelFetchMock.fetchModelsForConfig).not.toHaveBeenCalled();
  });

  it("1M 复选框联动：勾选后保存拼接 [1M] 后缀，未勾选不拼接", () => {
    const { onSave } = renderEditor();

    fillRequiredFields();
    fireEvent.change(targetModelInput(), {
      target: { value: "deepseek-v4-pro" },
    });

    // 未勾选：不拼接
    fireEvent.click(screen.getByText(SAVE));
    const [, route1] = onSave.mock.calls[0] as [ModelRole, ModelFamilyRoute];
    expect(route1.model).toBe("deepseek-v4-pro");

    // 勾选 1M 后：拼接
    const checkbox = screen.getByRole("checkbox");
    fireEvent.click(checkbox);
    fireEvent.click(screen.getByText(SAVE));
    const [, route2] = onSave.mock.calls[1] as [ModelRole, ModelFamilyRoute];
    expect(route2.model).toBe("deepseek-v4-pro[1M]");
  });

  it("获取模型列表失败时不泄漏原始错误对象", async () => {
    const consoleWarnSpy = vi
      .spyOn(console, "warn")
      .mockImplementation(() => {});
    modelFetchMock.fetchModelsForConfig.mockRejectedValue(
      new Error("sensitive upstream body"),
    );

    renderEditor();
    fillRequiredFields({ apiKey: "sk-abc" });
    fireEvent.click(screen.getByText(FETCH_MODELS));

    await waitFor(() => {
      expect(modelFetchMock.showFetchModelsError).toHaveBeenCalled();
    });
    const warnMessages = consoleWarnSpy.mock.calls.map((c) => String(c[0]));
    expect(warnMessages.join(" ")).toContain("[ModelFamilyRoute] Failed");
    expect(warnMessages.join(" ")).not.toContain("sensitive upstream body");
    consoleWarnSpy.mockRestore();
  });
});
