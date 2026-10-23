import { describe, expect, it } from "vitest";
import {
  ALL_CHILDREN_ID,
  createDefaultHudState,
  createHudStore,
  hudReducer,
  selectActiveL1Id,
  selectActiveL2Id,
  selectKeyHints,
  selectL2Panel,
  selectL3Bar,
  selectNotice,
  selectVisibleBallSlots,
  type HudNavigation,
  type HudState,
} from "./index";

const NAVIGATION: HudNavigation = {
  baseVersion: "1.1.0",
  effectiveVersion: "1.1.0+u2",
  l1s: [
    {
      id: "l1_inbox",
      name: "00 未分类",
      icon: "inbox",
      l2s: [{ id: "l2_manual_review", name: "待人工归档", selfBuilt: false }],
    },
    {
      id: "l1_content",
      name: "01 内容与表达",
      icon: "palette",
      l2s: [{ id: "l2_writing", name: "文本与写作", selfBuilt: false }],
    },
    {
      id: "l1_productivity",
      name: "02 效率与工作流",
      icon: "zap",
      l2s: [{ id: "l2_km", name: "信息与知识管理", selfBuilt: false }],
    },
    {
      id: "l1_tech",
      name: "03 技术与工程",
      icon: "terminal",
      l2s: [
        { id: "l2_prompt_agent", name: "提示工程与智能体", selfBuilt: false },
        { id: "l2_coding", name: "编程与低代码", selfBuilt: false },
        { id: "l2u_scripts", name: "交易脚本", selfBuilt: true },
      ],
    },
    {
      id: "l1_biz",
      name: "04 商业与认知",
      icon: "trending-up",
      l2s: [{ id: "l2_insight", name: "行业观察与研报", selfBuilt: false }],
    },
  ],
  formatTags: [
    { id: "tag_tool", name: "工具", color: "#3B82F6" },
    { id: "tag_tutorial", name: "教程", color: "#10B981" },
    { id: "tag_prompt", name: "提示词", color: "#8B5CF6" },
    { id: "tag_workflow", name: "工作流", color: "#EC4899" },
    { id: "tag_asset", name: "素材/资源", color: "#F59E0B" },
  ],
};

function stateWithNavigation(): HudState {
  return createDefaultHudState(NAVIGATION);
}

describe("球列来自生效分类树", () => {
  it("顺序为 搜索球 → L3 球 → 5 个 L1 球 → 快速录入球 → 设置球", () => {
    const slots = selectVisibleBallSlots(stateWithNavigation());
    expect(slots.map((slot) => slot.kind)).toEqual([
      "search",
      "l3-filter",
      "l1",
      "l1",
      "l1",
      "l1",
      "l1",
      "quick-add",
      "settings",
    ]);
    const l1Labels = slots.filter((slot) => slot.kind === "l1").map((slot) => slot.label);
    expect(l1Labels).toEqual([
      "00 未分类",
      "01 内容与表达",
      "02 效率与工作流",
      "03 技术与工程",
      "04 商业与认知",
    ]);
    expect(slots.filter((slot) => slot.kind === "l1")[3]?.l1Id).toBe("l1_tech");
  });

  it("搜索框展开后搜索球消失，顺序不变", () => {
    let state = stateWithNavigation();
    state = hudReducer(state, { type: "expand-search" });
    const kinds = selectVisibleBallSlots(state).map((slot) => slot.kind);
    expect(kinds[0]).toBe("l3-filter");
    expect(kinds).not.toContain("search");
  });

  it("导航尚未加载时保留 5 个占位 L1 球（壳启动失败/浏览器调试）", () => {
    const slots = selectVisibleBallSlots(createDefaultHudState());
    expect(slots.filter((slot) => slot.kind === "l1")).toHaveLength(5);
    expect(slots.filter((slot) => slot.kind === "l1").every((slot) => slot.placeholder)).toBe(
      true,
    );
  });

  it("set-navigation 替换导航数据；面板保留，父 L1 不存在时关闭", () => {
    const store = createHudStore();
    store.dispatch({ type: "set-navigation", navigation: NAVIGATION });
    expect(store.getState().navigation?.effectiveVersion).toBe("1.1.0+u2");
    store.dispatch({ type: "activate-l1", l1Id: "l1_tech" });

    store.dispatch({
      type: "set-navigation",
      navigation: { ...NAVIGATION, effectiveVersion: "1.1.0+u3" },
    });
    expect(selectL2Panel(store.getState())?.l1.id).toBe("l1_tech");

    store.dispatch({
      type: "set-navigation",
      navigation: {
        ...NAVIGATION,
        l1s: NAVIGATION.l1s.filter((l1) => l1.id !== "l1_tech"),
      },
    });
    expect(selectL2Panel(store.getState())).toBeNull();
  });
});

describe("L1 球原地展开 L2 面板", () => {
  it("点击 L1 打开面板，首项为「全部子类」，后接该类 L2（自建节点在末尾）", () => {
    let state = stateWithNavigation();
    state = hudReducer(state, { type: "activate-l1", l1Id: "l1_tech" });

    expect(selectActiveL1Id(state)).toBe("l1_tech");
    expect(selectActiveL2Id(state)).toBeNull();
    const panel = selectL2Panel(state);
    expect(panel?.l1.name).toBe("03 技术与工程");
    expect(panel?.items.map((item) => item.id)).toEqual([
      ALL_CHILDREN_ID,
      "l2_prompt_agent",
      "l2_coding",
      "l2u_scripts",
    ]);
    expect(panel?.items[0].isAllChildren).toBe(true);
    expect(panel?.items[0].name).toBe("全部子类");
    expect(panel?.items[3].selfBuilt).toBe(true);
  });

  it("再次点击同一 L1 收起面板；点击另一 L1 切换并清空 L2 选择", () => {
    let state = stateWithNavigation();
    state = hudReducer(state, { type: "activate-l1", l1Id: "l1_tech" });
    state = hudReducer(state, { type: "activate-l2", l2Id: "l2_coding" });
    expect(selectActiveL2Id(state)).toBe("l2_coding");

    state = hudReducer(state, { type: "activate-l1", l1Id: "l1_tech" });
    expect(selectL2Panel(state)).toBeNull();

    state = hudReducer(state, { type: "activate-l1", l1Id: "l1_tech" });
    state = hudReducer(state, { type: "activate-l2", l2Id: "l2_coding" });
    state = hudReducer(state, { type: "activate-l1", l1Id: "l1_biz" });
    expect(selectActiveL1Id(state)).toBe("l1_biz");
    expect(selectActiveL2Id(state)).toBeNull();
  });

  it("activate-l2 只接受当前面板里的节点；null 表示全部子类", () => {
    let state = stateWithNavigation();
    state = hudReducer(state, { type: "activate-l1", l1Id: "l1_tech" });
    state = hudReducer(state, { type: "activate-l2", l2Id: "l2_writing" });
    expect(selectActiveL2Id(state)).toBeNull();

    state = hudReducer(state, { type: "activate-l2", l2Id: "l2_coding" });
    expect(selectActiveL2Id(state)).toBe("l2_coding");
    state = hudReducer(state, { type: "activate-l2", l2Id: null });
    expect(selectActiveL2Id(state)).toBeNull();
  });

  it("未知 L1 与未加载导航时点击不产生面板", () => {
    let state = stateWithNavigation();
    state = hudReducer(state, { type: "activate-l1", l1Id: "l1_missing" });
    expect(selectL2Panel(state)).toBeNull();
    const empty = hudReducer(createDefaultHudState(), { type: "activate-l1", l1Id: "l1_tech" });
    expect(selectL2Panel(empty)).toBeNull();
  });
});

describe("L3 标签条（本票只渲染结构与顺序）", () => {
  it("点击 L3 球展开，形态标签保持生效树顺序；再次点击收起", () => {
    let state = stateWithNavigation();
    expect(selectL3Bar(state).expanded).toBe(false);
    state = hudReducer(state, { type: "toggle-l3-bar" });
    const bar = selectL3Bar(state);
    expect(bar.expanded).toBe(true);
    expect(bar.mediaChips.map((chip) => chip.name)).toEqual(["网页", "视频", "文章"]);
    expect(bar.formatTags.map((tag) => tag.id)).toEqual([
      "tag_tool",
      "tag_tutorial",
      "tag_prompt",
      "tag_workflow",
      "tag_asset",
    ]);
    state = hudReducer(state, { type: "toggle-l3-bar" });
    expect(selectL3Bar(state).expanded).toBe(false);
    state = hudReducer(state, { type: "toggle-l3-bar" });
    state = hudReducer(state, { type: "collapse-l3-bar" });
    expect(selectL3Bar(state).expanded).toBe(false);
  });
});

describe("启动提示条（5 秒由渲染层计时）", () => {
  it("显示/关闭提示，并可映射为中文文案", () => {
    let state = createDefaultHudState(NAVIGATION);
    state = hudReducer(state, {
      type: "show-notice",
      notice: {
        code: "taxonomy_overlay_corrupt",
        detail: "分类树覆盖层损坏，已备份为 taxonomy.overlay.corrupt-1.json 并按官方分类树启动",
      },
    });
    expect(selectNotice(state)?.code).toBe("taxonomy_overlay_corrupt");
    state = hudReducer(state, { type: "dismiss-notice" });
    expect(selectNotice(state)).toBeNull();
  });
});

describe("唤出重置交互态但保留导航数据", () => {
  it("清空搜索/查询/L2 面板/L3 条/提示，保留 navigation", () => {
    let state: HudState = createDefaultHudState(NAVIGATION);
    state = hudReducer(state, { type: "expand-search" });
    state = hudReducer(state, { type: "set-query", query: "rust" });
    state = hudReducer(state, { type: "activate-l1", l1Id: "l1_tech" });
    state = hudReducer(state, { type: "toggle-l3-bar" });
    state = hudReducer(state, {
      type: "show-notice",
      notice: { code: "taxonomy_overlay_corrupt", detail: "x" },
    });

    const summoned = hudReducer(state, { type: "summon" });
    expect(summoned.navigation?.effectiveVersion).toBe("1.1.0+u2");
    expect(summoned.search.expanded).toBe(false);
    expect(summoned.search.query).toBe("");
    expect(selectL2Panel(summoned)).toBeNull();
    expect(selectL3Bar(summoned).expanded).toBe(false);
    expect(selectNotice(summoned)).toBeNull();
    expect(summoned.phase).toBe("visible");
  });

  it("收起动画期间忽略面板/L3/提示命令，下次唤出重置", () => {
    let state = stateWithNavigation();
    state = hudReducer(state, { type: "dismiss-start" });
    expect(hudReducer(state, { type: "activate-l1", l1Id: "l1_tech" })).toBe(state);
    expect(hudReducer(state, { type: "toggle-l3-bar" })).toBe(state);
    expect(
      hudReducer(state, {
        type: "show-notice",
        notice: { code: "taxonomy_overlay_corrupt", detail: "x" },
      }),
    ).toBe(state);
    // 导航数据不是交互态，允许在收起期间更新（例如首帧晚于收起事件）。
    const updated = hudReducer(state, {
      type: "set-navigation",
      navigation: { ...NAVIGATION, effectiveVersion: "1.1.0+u3" },
    });
    expect(updated.navigation?.effectiveVersion).toBe("1.1.0+u3");
    expect(updated.phase).toBe("dismissing");
  });
});

describe("底部键位提示随焦点区变化", () => {
  it("搜索框、L2 面板、L3 条、球列各有提示", () => {
    let state = stateWithNavigation();
    expect(selectKeyHints(state).map((hint) => hint.label)).toContain("选择");

    const searching = hudReducer(state, { type: "expand-search" });
    expect(selectKeyHints(searching).map((hint) => hint.label)).toContain("进结果");

    state = hudReducer(state, { type: "activate-l1", l1Id: "l1_tech" });
    expect(selectKeyHints(state).map((hint) => hint.keys.join("+"))).toEqual(["Esc"]);
    expect(selectKeyHints(state).map((hint) => hint.label)).toEqual(["收起"]);

    const l3 = hudReducer(stateWithNavigation(), { type: "toggle-l3-bar" });
    expect(selectKeyHints(l3).map((hint) => hint.keys.join("+"))).toEqual(["Esc"]);
  });
});
