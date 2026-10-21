#!/usr/bin/env node
/**
 * AetherHUD 金标评测集校验器 —— #20 / docs/spec/classification-quality.md §3
 *
 * 用法：
 *   node scripts/eval/validate-eval-set.mjs --file <主集.jsonl> [--boundary <边界集.jsonl>]
 *        [--taxonomy <taxonomy.json>] [--eval-version <v>] [--json]
 *
 * 退出码：0 = 通过（可能有警告）；1 = 校验有错误；2 = 参数或读取失败。
 * 零依赖，要求 Node >= 18。
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_TAXONOMY = resolve(HERE, "../../taxonomy.json");

const RULES = {
  total: 200,
  visible: 150,
  blind: 50,
  l1Min: 40,
  l2Min: 10,
  hardRatio: 0.2,
  webRatio: 0.6,
  videoRatio: 0.2,
  textRatio: 0.1,
  bodyMax: 1500,
  formatMin: 1,
  formatMax: 2,
  featureMax: 3,
  featureLenMax: 20,
  blindShareMin: 0.15,
  blindShareMax: 0.35,
};

const ULID_RE = /^[0-9A-HJKMNP-TV-Z]{26}$/i;
const MEDIA_TYPES = new Set(["link", "video", "article"]);
const DIFFICULTIES = new Set(["easy", "hard"]);
const SPLITS = new Set(["visible", "blind"]);

function die(message) {
  console.error("参数错误：" + message);
  console.error("用 --help 查看用法。");
  process.exit(2);
}

function printHelp() {
  console.log([
    "AetherHUD 金标评测集校验器",
    "",
    "用法：",
    "  node scripts/eval/validate-eval-set.mjs --file <主集.jsonl> [选项]",
    "",
    "选项：",
    "  --file <path>           主评测集 JSONL（visible + blind 同文件，按 split 字段区分）【必填】",
    "  --boundary <path>       边界集 JSONL（选填；只做格式与引用校验，不参与配额）",
    "  --taxonomy <path>       taxonomy.json 路径（默认 ../../taxonomy.json）",
    "  --eval-version <v>      期望的 eval_set_version（如 2026-10-v1；默认以文件内记录为准）",
    "  --json                  输出机器可读 JSON",
    "  -h, --help              显示本帮助",
    "",
    "校验内容（#14 §3）：",
    "  · 字段完整性与取值合法性（gold.l1/l2 必须是正常节点，形态标签 1~2 个，特征标签 0~3 个）",
    "  · 总量 200 = visible 150 + blind 50",
    "  · 12 个正常 L2 每个 >= 10、4 个正常 L1 每个 >= 40",
    "  · 难例 >= 20%；网页 >= 60% / 视频 >= 20% / 纯文本 >= 10%",
    "  · taxonomy_version 与 taxonomy.json 一致；eval_set_version 单一",
    "  · entry_id 唯一；URL 重复告警",
  ].join("\n"));
}

function parseArgs(argv) {
  const opts = { taxonomy: DEFAULT_TAXONOMY, json: false, file: null, boundary: null, evalVersion: null, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") { opts.help = true; continue; }
    if (arg === "--json") { opts.json = true; continue; }
    const value = () => {
      const v = argv[i + 1];
      if (v === undefined || v.startsWith("--")) die(arg + " 需要一个值");
      i += 1;
      return v;
    };
    if (arg === "--file") opts.file = value();
    else if (arg === "--boundary") opts.boundary = value();
    else if (arg === "--taxonomy") opts.taxonomy = value();
    else if (arg === "--eval-version") opts.evalVersion = value();
    else die("未知参数 " + arg);
  }
  return opts;
}

function loadTaxonomy(path) {
  const doc = JSON.parse(readFileSync(path, "utf8"));
  const normalL1 = new Map();
  const normalL2 = new Map();
  const allL1 = new Set();
  const allL2 = new Map();
  for (const l1 of doc.taxonomy || []) {
    allL1.add(l1.id);
    const normal = l1.id !== "l1_inbox";
    if (normal) normalL1.set(l1.id, { name: l1.name });
    for (const child of l1.children || []) {
      allL2.set(child.id, { l1: l1.id, name: child.name, normal });
      if (normal) normalL2.set(child.id, l1.id);
    }
  }
  const formatTags = new Set(((doc.global_l3_tags || {}).format_tags || []).map((t) => t.id));
  return { version: doc.version, normalL1, normalL2, allL1, allL2, formatTags };
}

function readJsonl(path) {
  const text = readFileSync(path, "utf8");
  const records = [];
  const parseErrors = [];
  const lines = text.split(/\r?\n/);
  lines.forEach((raw, index) => {
    const line = index + 1;
    if (raw.trim() === "") return;
    try {
      records.push({ line, value: JSON.parse(raw) });
    } catch (err) {
      parseErrors.push("第 " + line + " 行：JSON 解析失败（" + err.message + "）");
    }
  });
  return { records, parseErrors };
}

const isString = (v) => typeof v === "string";
const has = (obj, key) => obj !== null && typeof obj === "object" && Object.prototype.hasOwnProperty.call(obj, key);
const charLen = (s) => [...s].length;

function checkFormatTags(value, tax, errors, line) {
  if (!Array.isArray(value)) { errors.push("第 " + line + " 行：gold.format_tags 必须是数组"); return; }
  if (value.length < RULES.formatMin || value.length > RULES.formatMax) {
    errors.push("第 " + line + " 行：gold.format_tags 必须 1~2 个（当前 " + value.length + " 个）");
  }
  const seen = new Set();
  for (const tag of value) {
    if (!isString(tag)) { errors.push("第 " + line + " 行：gold.format_tags 含非字符串元素"); continue; }
    if (!tax.formatTags.has(tag)) errors.push("第 " + line + " 行：形态标签 " + JSON.stringify(tag) + " 不在 taxonomy format_tags 中");
    if (seen.has(tag)) errors.push("第 " + line + " 行：gold.format_tags 重复 " + JSON.stringify(tag));
    seen.add(tag);
  }
}

function checkFeatureTags(value, errors, line) {
  if (!Array.isArray(value)) { errors.push("第 " + line + " 行：gold.feature_tags 必须是数组"); return; }
  if (value.length > RULES.featureMax) errors.push("第 " + line + " 行：gold.feature_tags 最多 3 个（当前 " + value.length + " 个）");
  const seen = new Set();
  for (const tag of value) {
    if (!isString(tag)) { errors.push("第 " + line + " 行：gold.feature_tags 含非字符串元素"); continue; }
    if (charLen(tag) < 1 || charLen(tag) > RULES.featureLenMax) {
      errors.push("第 " + line + " 行：特征标签 " + JSON.stringify(tag) + " 长度须为 1~20 字");
    }
    if (seen.has(tag)) errors.push("第 " + line + " 行：gold.feature_tags 重复 " + JSON.stringify(tag));
    seen.add(tag);
  }
}

function validateGold(gold, tax, errors, warnings, line) {
  if (gold === null || typeof gold !== "object" || Array.isArray(gold)) {
    errors.push("第 " + line + " 行：gold 必须是对象");
    return;
  }
  if (!isString(gold.l1_id)) errors.push("第 " + line + " 行：gold.l1_id 缺失或不是字符串");
  else if (!tax.normalL1.has(gold.l1_id)) {
    errors.push("第 " + line + " 行：gold.l1_id " + JSON.stringify(gold.l1_id) + " 不是 taxonomy 的正常 L1（主评测集不允许 l1_inbox）");
  }
  if (!isString(gold.l2_id)) errors.push("第 " + line + " 行：gold.l2_id 缺失或不是字符串");
  else {
    const parent = tax.normalL2.get(gold.l2_id);
    if (!parent) errors.push("第 " + line + " 行：gold.l2_id " + JSON.stringify(gold.l2_id) + " 不是 taxonomy 的正常 L2（主评测集不允许 l2_manual_review）");
    else if (isString(gold.l1_id) && parent !== gold.l1_id) {
      errors.push("第 " + line + " 行：gold.l2_id " + JSON.stringify(gold.l2_id) + " 不属于 gold.l1_id " + JSON.stringify(gold.l1_id));
    }
  }
  if (!has(gold, "format_tags")) errors.push("第 " + line + " 行：gold.format_tags 缺失");
  else checkFormatTags(gold.format_tags, tax, errors, line);
  if (!has(gold, "feature_tags")) errors.push("第 " + line + " 行：gold.feature_tags 缺失");
  else checkFeatureTags(gold.feature_tags, errors, line);
}

function mediaClass(record) {
  if (record.media_type === "video") return "video";
  if (!isString(record.url) || record.url.trim() === "") return "text";
  return "web";
}

function newStats() {
  return {
    total: 0,
    byL1: new Map(),
    byL2: new Map(),
    difficulty: { easy: 0, hard: 0, other: 0 },
    media: { web: 0, video: 0, text: 0, other: 0 },
    split: { visible: 0, blind: 0, other: 0 },
    evalVersions: new Map(),
    taxonomyVersions: new Map(),
    entryIds: new Map(),
    urls: new Map(),
    bodyLengths: [],
  };
}

function bump(map, key, line) {
  if (!map.has(key)) map.set(key, []);
  map.get(key).push(line);
}

function validateMainSet(records, tax, opts, errors, warnings, stats) {
  for (const item of records) {
    const line = item.line;
    const record = item.value;
    if (record === null || typeof record !== "object" || Array.isArray(record)) {
      errors.push("第 " + line + " 行：记录必须是 JSON 对象");
      continue;
    }
    for (const field of ["entry_id", "title", "url", "body_excerpt", "media_type", "difficulty", "split", "eval_set_version", "taxonomy_version", "notes"]) {
      if (!has(record, field)) errors.push("第 " + line + " 行：缺少字段 " + field);
    }
    if (isString(record.entry_id) && record.entry_id.trim() !== "") {
      if (!ULID_RE.test(record.entry_id)) warnings.push("第 " + line + " 行：entry_id 不是标准 ULID（" + record.entry_id + "），请确认与库内 uid 对齐");
      if (stats.entryIds.has(record.entry_id)) errors.push("第 " + line + " 行：entry_id 重复（首见第 " + stats.entryIds.get(record.entry_id) + " 行）");
      else stats.entryIds.set(record.entry_id, line);
    } else errors.push("第 " + line + " 行：entry_id 缺失或为空");

    if (!isString(record.title) || record.title.trim() === "") errors.push("第 " + line + " 行：title 缺失或为空");
    if (!isString(record.url)) errors.push("第 " + line + " 行：url 必须是字符串（纯文本条目用空字符串）");
    if (!isString(record.body_excerpt)) errors.push("第 " + line + " 行：body_excerpt 必须是字符串");
    else {
      const len = charLen(record.body_excerpt);
      stats.bodyLengths.push(len);
      if (len > RULES.bodyMax) errors.push("第 " + line + " 行：body_excerpt 超过 1500 字（当前 " + len + "）");
      if (len === 0 && record.difficulty !== "hard") warnings.push("第 " + line + " 行：body_excerpt 为空但不是 hard 难度（仅标题级难例应留空）");
    }
    if (!MEDIA_TYPES.has(record.media_type)) errors.push("第 " + line + " 行：media_type 非法（允许 link/video/article）");
    if (!has(record, "gold")) errors.push("第 " + line + " 行：缺少 gold");
    else validateGold(record.gold, tax, errors, warnings, line);
    if (!DIFFICULTIES.has(record.difficulty)) errors.push("第 " + line + " 行：difficulty 非法（允许 easy/hard）");
    if (!SPLITS.has(record.split)) errors.push("第 " + line + " 行：split 非法（主评测集允许 visible/blind）");
    if (isString(record.eval_set_version)) {
      if (opts.evalVersion && record.eval_set_version !== opts.evalVersion) {
        errors.push("第 " + line + " 行：eval_set_version " + JSON.stringify(record.eval_set_version) + " 与期望 " + JSON.stringify(opts.evalVersion) + " 不一致");
      }
      bump(stats.evalVersions, record.eval_set_version, line);
    }
    if (isString(record.taxonomy_version)) {
      if (record.taxonomy_version !== tax.version) {
        errors.push("第 " + line + " 行：taxonomy_version " + JSON.stringify(record.taxonomy_version) + " 与 taxonomy.json 的 " + JSON.stringify(tax.version) + " 不一致");
      }
      bump(stats.taxonomyVersions, record.taxonomy_version, line);
    }
    if (!isString(record.notes)) errors.push("第 " + line + " 行：notes 必须是字符串（可空字符串）");

    if (SPLITS.has(record.split) && record.gold && isString(record.gold.l1_id) && isString(record.gold.l2_id)) {
      stats.total += 1;
      stats.split[record.split] += 1;
      stats.difficulty[DIFFICULTIES.has(record.difficulty) ? record.difficulty : "other"] += 1;
      stats.media[mediaClass(record)] += 1;
      if (!stats.byL1.has(record.gold.l1_id)) stats.byL1.set(record.gold.l1_id, { total: 0, visible: 0, blind: 0 });
      stats.byL1.get(record.gold.l1_id).total += 1;
      stats.byL1.get(record.gold.l1_id)[record.split] += 1;
      if (!stats.byL2.has(record.gold.l2_id)) stats.byL2.set(record.gold.l2_id, 0);
      stats.byL2.set(record.gold.l2_id, stats.byL2.get(record.gold.l2_id) + 1);
      if (isString(record.url) && record.url.trim() !== "") {
        if (stats.urls.has(record.url)) warnings.push("第 " + line + " 行：URL 与第 " + stats.urls.get(record.url) + " 行重复（" + record.url + "）");
        else stats.urls.set(record.url, line);
      }
    }
  }
}

function validateQuotas(tax, stats, errors, warnings) {
  if (stats.total !== RULES.total) errors.push("总量 " + stats.total + " 条，要求 " + RULES.total + " 条");
  if (stats.split.visible !== RULES.visible) errors.push("visible " + stats.split.visible + " 条，要求 " + RULES.visible + " 条");
  if (stats.split.blind !== RULES.blind) errors.push("blind " + stats.split.blind + " 条，要求 " + RULES.blind + " 条");

  for (const entry of tax.normalL1) {
    const l1Id = entry[0];
    const meta = entry[1];
    const count = stats.byL1.has(l1Id) ? stats.byL1.get(l1Id).total : 0;
    if (count < RULES.l1Min) errors.push("L1 " + l1Id + "（" + meta.name + "）只有 " + count + " 条，要求 >= " + RULES.l1Min);
    const row = stats.byL1.get(l1Id);
    if (row && row.total > 0) {
      const share = row.blind / row.total;
      if (share < RULES.blindShareMin || share > RULES.blindShareMax) {
        warnings.push("L1 " + l1Id + " 的盲测占比 " + (share * 100).toFixed(1) + "%，偏离分层目标 ~25%（建议 15%~35%）");
      }
      if (row.blind === 0) warnings.push("L1 " + l1Id + " 没有盲测条目");
    }
  }
  for (const entry of tax.normalL2) {
    const l2Id = entry[0];
    const count = stats.byL2.has(l2Id) ? stats.byL2.get(l2Id) : 0;
    if (count < RULES.l2Min) {
      const meta = tax.allL2.get(l2Id);
      errors.push("L2 " + l2Id + "（" + (meta ? meta.name : "") + "）只有 " + count + " 条，要求 >= " + RULES.l2Min);
    }
  }

  const total = stats.total || 1;
  const hardShare = stats.difficulty.hard / total;
  if (hardShare < RULES.hardRatio) errors.push("难例比例 " + (hardShare * 100).toFixed(1) + "%，要求 >= 20%");
  const webShare = stats.media.web / total;
  const videoShare = stats.media.video / total;
  const textShare = stats.media.text / total;
  if (webShare < RULES.webRatio) errors.push("网页占比 " + (webShare * 100).toFixed(1) + "%，要求 >= 60%");
  if (videoShare < RULES.videoRatio) errors.push("视频占比 " + (videoShare * 100).toFixed(1) + "%，要求 >= 20%");
  if (textShare < RULES.textRatio) errors.push("纯文本占比 " + (textShare * 100).toFixed(1) + "%，要求 >= 10%");

  if (stats.evalVersions.size > 1) errors.push("文件内出现多个 eval_set_version：" + [...stats.evalVersions.keys()].join(", ") + "（单文件只能属于一个版本）");
  if (stats.taxonomyVersions.size > 1) errors.push("文件内出现多个 taxonomy_version：" + [...stats.taxonomyVersions.keys()].join(", "));
}

function validateBoundary(records, tax, errors) {
  let count = 0;
  for (const item of records) {
    const line = item.line;
    const record = item.value;
    if (record === null || typeof record !== "object" || Array.isArray(record)) {
      errors.push("边界集第 " + line + " 行：记录必须是 JSON 对象");
      continue;
    }
    count += 1;
    for (const field of ["entry_id", "title", "url", "body_excerpt", "media_type"]) {
      if (!has(record, field)) errors.push("边界集第 " + line + " 行：缺少字段 " + field);
    }
    if (!isString(record.title) || record.title.trim() === "") errors.push("边界集第 " + line + " 行：title 缺失或为空");
    if (!isString(record.url)) errors.push("边界集第 " + line + " 行：url 必须是字符串");
    if (!MEDIA_TYPES.has(record.media_type)) errors.push("边界集第 " + line + " 行：media_type 非法");
    if (has(record, "split") && record.split !== "boundary") errors.push("边界集第 " + line + " 行：split 必须为 boundary");
    if (!isString(record.notes) || record.notes.trim() === "") errors.push("边界集第 " + line + " 行：必须用 notes 记录为什么进边界集");
    if (has(record, "gold") && record.gold !== null) {
      const gold = record.gold;
      if (typeof gold !== "object" || Array.isArray(gold)) errors.push("边界集第 " + line + " 行：gold 必须是对象或 null");
      else {
        if (has(gold, "l1_id") && gold.l1_id !== null && !tax.allL1.has(gold.l1_id)) errors.push("边界集第 " + line + " 行：gold.l1_id " + JSON.stringify(gold.l1_id) + " 不存在");
        if (has(gold, "l2_id") && gold.l2_id !== null && !tax.allL2.has(gold.l2_id)) errors.push("边界集第 " + line + " 行：gold.l2_id " + JSON.stringify(gold.l2_id) + " 不存在");
        if (has(gold, "format_tags")) {
          if (!Array.isArray(gold.format_tags)) errors.push("边界集第 " + line + " 行：gold.format_tags 必须是数组");
          else for (const tag of gold.format_tags) if (!tax.formatTags.has(tag)) errors.push("边界集第 " + line + " 行：形态标签 " + JSON.stringify(tag) + " 不在 taxonomy 中");
        }
      }
    }
  }
  return count;
}

function pct(part, total) {
  if (!total) return "0.0%";
  return ((part / total) * 100).toFixed(1) + "%";
}

function render(ok, file, boundary, tax, stats, errors, warnings, boundaryCount) {
  const lines = [];
  lines.push("== AetherHUD 金标评测集校验 ==");
  lines.push("主集文件：" + file);
  if (boundary) lines.push("边界集文件：" + boundary);
  lines.push("taxonomy：" + tax.version + "（正常 L1 " + tax.normalL1.size + " 个 / 正常 L2 " + tax.normalL2.size + " 个 / 形态标签 " + tax.formatTags.size + " 个）");
  lines.push("");
  lines.push("总量：" + stats.total + " 条（visible " + stats.split.visible + " / blind " + stats.split.blind + "）");
  lines.push("难度：easy " + stats.difficulty.easy + " / hard " + stats.difficulty.hard + "（" + pct(stats.difficulty.hard, stats.total) + "，门线 >= 20%）");
  lines.push("媒体：网页 " + stats.media.web + "（" + pct(stats.media.web, stats.total) + "）/ 视频 " + stats.media.video + "（" + pct(stats.media.video, stats.total) + "）/ 纯文本 " + stats.media.text + "（" + pct(stats.media.text, stats.total) + "）");
  lines.push("");
  lines.push("L1 分布：");
  for (const entry of tax.normalL1) {
    const l1Id = entry[0];
    const meta = entry[1];
    const row = stats.byL1.get(l1Id) || { total: 0, visible: 0, blind: 0 };
    lines.push("  " + l1Id.padEnd(15, " ") + meta.name.padEnd(12, " ") + " 共 " + String(row.total).padStart(3, " ") + "（visible " + row.visible + " / blind " + row.blind + "，门线 >= 40）");
  }
  lines.push("L2 分布：");
  for (const entry of tax.normalL2) {
    const l2Id = entry[0];
    const meta = tax.allL2.get(l2Id);
    const count = stats.byL2.has(l2Id) ? stats.byL2.get(l2Id) : 0;
    lines.push("  " + l2Id.padEnd(20, " ") + (meta ? meta.name : "").padEnd(12, " ") + " " + String(count).padStart(3, " ") + "（门线 >= 10）");
  }
  const avgBody = stats.bodyLengths.length ? Math.round(stats.bodyLengths.reduce((a, b) => a + b, 0) / stats.bodyLengths.length) : 0;
  lines.push("");
  lines.push("body_excerpt 平均长度：" + avgBody + " 字（上限 1500）");
  if (stats.evalVersions.size) lines.push("eval_set_version：" + [...stats.evalVersions.keys()].join(", "));
  if (boundary) lines.push("边界集：" + boundaryCount + " 条（不参与门线）");
  lines.push("");
  if (errors.length) {
    lines.push("错误（" + errors.length + "）：");
    for (const e of errors) lines.push("  - " + e);
  }
  if (warnings.length) {
    lines.push("警告（" + warnings.length + "）：");
    for (const w of warnings) lines.push("  - " + w);
  }
  lines.push("");
  lines.push("结果：" + (ok ? "PASS" : "FAIL"));
  return lines.join("\n");
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) { printHelp(); return; }
  if (!opts.file) die("缺少 --file");

  let tax;
  let mainFile;
  try {
    tax = loadTaxonomy(opts.taxonomy);
    mainFile = readJsonl(opts.file);
  } catch (err) {
    console.error("读取失败：" + err.message);
    process.exit(2);
  }

  const errors = [...mainFile.parseErrors];
  const warnings = [];
  const stats = newStats();
  validateMainSet(mainFile.records, tax, opts, errors, warnings, stats);
  validateQuotas(tax, stats, errors, warnings);

  let boundaryCount = 0;
  if (opts.boundary) {
    try {
      const boundaryFile = readJsonl(opts.boundary);
      errors.push(...boundaryFile.parseErrors.map((e) => e.replace("第 ", "边界集第 ")));
      boundaryCount = validateBoundary(boundaryFile.records, tax, errors);
    } catch (err) {
      console.error("边界集读取失败：" + err.message);
      process.exit(2);
    }
  }

  const ok = errors.length === 0;
  if (opts.json) {
    console.log(JSON.stringify({
      ok,
      file: resolve(opts.file),
      boundary: opts.boundary ? resolve(opts.boundary) : null,
      taxonomyVersion: tax.version,
      total: stats.total,
      split: stats.split,
      difficulty: stats.difficulty,
      media: stats.media,
      byL1: Object.fromEntries([...stats.byL1.entries()]),
      byL2: Object.fromEntries([...stats.byL2.entries()]),
      evalSetVersions: [...stats.evalVersions.keys()],
      boundaryCount,
      errors,
      warnings,
    }, null, 2));
  } else {
    console.log(render(ok, resolve(opts.file), opts.boundary ? resolve(opts.boundary) : null, tax, stats, errors, warnings, boundaryCount));
  }
  process.exit(ok ? 0 : 1);
}

main();
