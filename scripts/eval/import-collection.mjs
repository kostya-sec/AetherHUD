#!/usr/bin/env node
/**
 * 收藏导出 → 评测候选 JSONL（#20 标注前置）
 *
 * 支持：Chrome/Edge Bookmarks JSON、通用 JSON 数组、Netscape 书签 HTML、
 *       CSV（含 url/title 或 链接/标题 表头）、TXT/MD（一行一个 URL 或 Markdown 链接）。
 *
 * 用法：
 *   node scripts/eval/import-collection.mjs --in <file> [--in <file> ...] --out <candidates.jsonl>
 *        [--limit N] [--json]
 *
 * 输出条目仅含候选字段（entry_id/title/url/media_type/source/notes），
 * gold/difficulty/split 由标注阶段填写；不可直接作为评测集。
 * 真实收藏内容只写本地 --out，不入 git。
 */
import crypto from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { extname, resolve } from "node:path";

const HTTP_RE = /^https?:\/\//i;
const VIDEO_HOSTS = new Set(["youtube.com", "www.youtube.com", "m.youtube.com", "youtu.be", "bilibili.com", "www.bilibili.com", "b23.tv", "vimeo.com"]);
const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

function die(message) {
  console.error("参数错误：" + message);
  console.error("用 --help 查看用法。");
  process.exit(2);
}

function printHelp() {
  console.log([
    "收藏导出 → 评测候选 JSONL（#20）",
    "",
    "用法：",
    "  node scripts/eval/import-collection.mjs --in <file> [--in <file> ...] --out <candidates.jsonl> [--limit N] [--json]",
    "",
    "支持格式：Chrome/Edge Bookmarks JSON、通用 JSON、书签 HTML、CSV、TXT/MD。",
    "自动去重（canonical URL）并推断 media_type（视频站 → video，其余 → link）。",
  ].join("\n"));
}

function parseArgs(argv) {
  const opts = { inputs: [], out: null, limit: 0, json: false, help: false };
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
    if (arg === "--in") opts.inputs.push(value());
    else if (arg === "--out") opts.out = value();
    else if (arg === "--limit") opts.limit = Number(value());
    else die("未知参数 " + arg);
  }
  return opts;
}

const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", "#39": "'", nbsp: " " };
function decodeEntities(text) {
  return String(text).replace(/&(#x?[0-9a-f]+|[a-z]+);/gi, (whole, name) => {
    const key = name.toLowerCase();
    if (key in ENTITIES) return ENTITIES[key];
    if (key.startsWith("#x")) return String.fromCodePoint(parseInt(key.slice(2), 16) || 32);
    if (key.startsWith("#")) return String.fromCodePoint(parseInt(key.slice(1), 10) || 32);
    return whole;
  });
}

function stripHtml(text) {
  return decodeEntities(String(text).replace(/<[^>]*>/g, " ")).replace(/\s+/g, " ").trim();
}

function ulid() {
  let ts = Date.now();
  let time = "";
  for (let i = 0; i < 10; i += 1) { time = CROCKFORD[ts % 32] + time; ts = Math.floor(ts / 32); }
  const bytes = crypto.randomBytes(16);
  let rand = "";
  for (let i = 0; i < 16; i += 1) rand += CROCKFORD[bytes[i] % 32];
  return time + rand;
}

function canonicalUrl(raw) {
  const input = String(raw || "").trim();
  if (!HTTP_RE.test(input)) return null;
  try {
    const u = new URL(input);
    u.hash = "";
    const drop = [];
    u.searchParams.forEach((_, key) => { if (/^utm_/i.test(key) || /^(spm|from|share_source|vd_source)$/i.test(key)) drop.push(key); });
    for (const key of drop) u.searchParams.delete(key);
    u.hostname = u.hostname.toLowerCase();
    let out = u.toString();
    if (out.endsWith("/") && u.pathname !== "/") out = out.slice(0, -1);
    return out;
  } catch {
    return null;
  }
}

function inferMedia(url) {
  try {
    const host = new URL(url).hostname.toLowerCase();
    if (VIDEO_HOSTS.has(host) || VIDEO_HOSTS.has(host.replace(/^www\./, ""))) return "video";
  } catch { /* ignore */ }
  return "link";
}

function extractUrlsFromHtml(text) {
  const items = [];
  const anchorRe = /<a\b[^>]*href\s*=\s*("([^"]*)"|'([^']*)')[^>]*>([\s\S]*?)<\/a>/gi;
  let match;
  while ((match = anchorRe.exec(text)) !== null) {
    const href = decodeEntities(match[2] !== undefined ? match[2] : match[3]);
    const url = canonicalUrl(href);
    if (!url) continue;
    items.push({ title: stripHtml(match[4]) || url, url });
  }
  return items;
}

function extractChromeBookmarks(node, out) {
  if (!node || typeof node !== "object") return;
  if (node.type === "url" && typeof node.url === "string") {
    const url = canonicalUrl(node.url);
    if (url) out.push({ title: String(node.name || url), url });
  }
  const children = Array.isArray(node.children) ? node.children : [];
  for (const child of children) extractChromeBookmarks(child, out);
}

const TITLE_KEYS = ["title", "name", "标题", "名称", "text", "label"];
const URL_KEYS = ["url", "link", "href", "链接", "网址", "地址"];
function pickKey(obj, keys) {
  for (const key of keys) if (typeof obj[key] === "string" && obj[key].trim() !== "") return obj[key].trim();
  return null;
}
function itemsFromObjects(list, source) {
  const out = [];
  for (const obj of list) {
    if (!obj || typeof obj !== "object") continue;
    const rawUrl = pickKey(obj, URL_KEYS);
    const url = rawUrl ? canonicalUrl(rawUrl) : null;
    if (!url) continue;
    const title = pickKey(obj, TITLE_KEYS) || url;
    out.push({ title, url, source });
  }
  return out;
}

function parseCsvLine(line) {
  const cells = [];
  let current = "";
  let quoted = false;
  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (quoted) {
      if (ch === '"' && line[i + 1] === '"') { current += '"'; i += 1; }
      else if (ch === '"') quoted = false;
      else current += ch;
    } else if (ch === '"') quoted = true;
    else if (ch === ",") { cells.push(current); current = ""; }
    else current += ch;
  }
  cells.push(current);
  return cells;
}

function parseCsv(text, source) {
  const rows = text.split(/\r?\n/).filter((line) => line.trim() !== "").map(parseCsvLine);
  if (!rows.length) return [];
  const header = rows[0].map((cell) => cell.trim().toLowerCase());
  const urlIndex = header.findIndex((cell) => ["url", "link", "href", "链接", "网址", "地址"].includes(cell));
  const titleIndex = header.findIndex((cell) => ["title", "name", "标题", "名称"].includes(cell));
  if (urlIndex === -1) return [];
  const out = [];
  for (const row of rows.slice(1)) {
    const url = canonicalUrl(row[urlIndex] || "");
    if (!url) continue;
    out.push({ title: (titleIndex >= 0 ? row[titleIndex] : "") || url, url, source });
  }
  return out;
}

function parseTextLines(text, source) {
  const out = [];
  const mdRe = /\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g;
  const seen = new Set();
  let match;
  while ((match = mdRe.exec(text)) !== null) {
    const url = canonicalUrl(match[2]);
    if (url && !seen.has(url)) { seen.add(url); out.push({ title: match[1].trim() || url, url, source }); }
  }
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    const urlMatch = line.match(/https?:\/\/[^\s)<>"']+/);
    if (!urlMatch) {
      if (line.length <= 120) { /* 纯文本候选由标注阶段手工补 URL，不在此自动收集 */ }
      continue;
    }
    const url = canonicalUrl(urlMatch[0]);
    if (!url || seen.has(url)) continue;
    seen.add(url);
    const title = line.replace(urlMatch[0], "").replace(/^[\s\-–—|:：]+/, "").replace(/[\s\-–—|:：]+$/, "").trim() || url;
    out.push({ title, url, source });
  }
  return out;
}

function parseFile(file) {
  const text = readFileSync(file, "utf8");
  const ext = extname(file).toLowerCase();
  const source = resolve(file);
  if (ext === ".html" || ext === ".htm") return extractUrlsFromHtml(text);
  if (ext === ".csv") return parseCsv(text, source);
  if (ext === ".json") {
    const doc = JSON.parse(text);
    const out = [];
    if (doc && typeof doc === "object" && doc.roots && typeof doc.roots === "object") {
      for (const root of Object.values(doc.roots)) extractChromeBookmarks(root, out);
      return out.map((item) => ({ ...item, source }));
    }
    const list = Array.isArray(doc) ? doc : Array.isArray(doc.items) ? doc.items : Array.isArray(doc.records) ? doc.records : [];
    return itemsFromObjects(list, source);
  }
  if (ext === ".jsonl" || ext === ".ndjson") {
    const out = [];
    for (const line of text.split(/\r?\n/)) {
      if (!line.trim()) continue;
      try { out.push(JSON.parse(line)); } catch { /* skip */ }
    }
    return itemsFromObjects(out, source);
  }
  return parseTextLines(text, source);
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) { printHelp(); return; }
  if (!opts.inputs.length) die("缺少 --in");
  if (!opts.out) die("缺少 --out");

  const all = [];
  const stats = { parsed: 0, invalid: 0, duplicates: 0, files: [] };
  const seen = new Set();
  for (const file of opts.inputs) {
    let items;
    try {
      items = parseFile(file);
    } catch (err) {
      console.error("读取失败 " + file + "：" + err.message);
      process.exit(2);
    }
    stats.files.push({ file: resolve(file), count: items.length });
    stats.parsed += items.length;
    for (const item of items) {
      const url = item.url;
      if (seen.has(url)) { stats.duplicates += 1; continue; }
      seen.add(url);
      all.push({
        entry_id: ulid(),
        title: item.title || url,
        url,
        media_type: inferMedia(url),
        source: item.source || resolve(file),
        notes: "",
      });
      if (opts.limit && all.length >= opts.limit) break;
    }
    if (opts.limit && all.length >= opts.limit) break;
  }

  writeFileSync(opts.out, all.map((item) => JSON.stringify(item)).join("\n") + (all.length ? "\n" : ""), "utf8");
  const byMedia = all.reduce((acc, item) => { acc[item.media_type] = (acc[item.media_type] || 0) + 1; return acc; }, {});
  const result = {
    ok: true,
    out: resolve(opts.out),
    total: all.length,
    byMedia,
    duplicates: stats.duplicates,
    files: stats.files,
  };
  if (opts.json) console.log(JSON.stringify(result, null, 2));
  else {
    console.log("== 收藏导入 ==");
    for (const file of stats.files) console.log("  " + file.file + " → " + file.count + " 条");
    console.log("唯一候选：" + all.length + " 条（去重 " + stats.duplicates + " 条）");
    console.log("媒体推断：link " + (byMedia.link || 0) + " / video " + (byMedia.video || 0));
    console.log("输出：" + resolve(opts.out));
    console.log("提示：gold / difficulty / split 需标注阶段补齐；候选文件不得直接当评测集。");
  }
}

main();
