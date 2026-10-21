#!/usr/bin/env node
/**
 * verify-keys.mjs -- AetherHUD 凭证连通性验证（零依赖，Node 18+）
 *
 * 用法:
 *   node scripts/setup/verify-keys.mjs                 # 读 .local/keys.env
 *   node scripts/setup/verify-keys.mjs --env <path>    # 读指定文件
 *   node scripts/setup/verify-keys.mjs --only jina     # 只跑某一家 (jina|youtube|feishu)
 *
 * 做了什么:
 *   Jina   -> 真抓一次 https://r.jina.ai/https://example.com，读 x-ratelimit-* 头
 *   YouTube-> videos.list 查一个公开视频（花 1 单位配额）
 *   飞书   -> tenant_access_token -> 列数据表 -> 读字段 -> search 读 5 条记录
 *
 * 产物:
 *   - 控制台摘要（英文，避免 Windows 控制台乱码）
 *   - .local/keys-report.json（实测结果，不含密钥明文）
 *
 * 退出码: 0 = 配置项全部通过; 1 = 至少一项失败; 2 = 参数/文件错误
 */

import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';

const DEFAULT_ENV = '.local/keys.env';
const REPORT_PATH = '.local/keys-report.json';

/* ----------------------------- 参数与 env ----------------------------- */

function parseArgs(argv) {
  const out = { env: DEFAULT_ENV, only: null, help: false };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--env') out.env = argv[++i];
    else if (argv[i] === '--only') out.only = argv[++i];
    else if (argv[i] === '--help' || argv[i] === '-h') out.help = true;
  }
  return out;
}

function parseEnvFile(text) {
  const out = {};
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) continue;
    const eq = line.indexOf('=');
    if (eq < 0) continue;
    const key = line.slice(0, eq).trim();
    let val = line.slice(eq + 1).trim();
    if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
      val = val.slice(1, -1);
    }
    if (key) out[key] = val;
  }
  return out;
}

const args = parseArgs(process.argv.slice(2));
const envPath = resolve(args.env);
if (!existsSync(envPath)) {
  console.error('[FAIL] env file not found: ' + envPath);
  console.error('       copy the template from docs/spec/api-keys.md into that path first.');
  process.exit(2);
}
const env = parseEnvFile(readFileSync(envPath, 'utf8'));

const pick = (...names) => {
  for (const n of names) if (env[n] && env[n].trim()) return env[n].trim();
  return '';
};

const cfg = {
  jinaKey: pick('JINA_API_KEY', 'JINA_KEY'),
  youtubeKey: pick('YOUTUBE_API_KEY', 'YT_API_KEY'),
  feishuAppId: pick('FEISHU_APP_ID'),
  feishuAppSecret: pick('FEISHU_APP_SECRET'),
  feishuBaseUrl: pick('FEISHU_BASE_URL', 'FEISHU_BITABLE_URL'),
};

/* ------------------------------- helpers ------------------------------ */

const results = {};
const startedAt = new Date().toISOString();
const log = (...a) => console.log(...a);

function mask(v) {
  if (!v) return '(missing)';
  if (v.length <= 8) return '***';
  return v.slice(0, 4) + '...' + v.slice(-4) + ' (len=' + v.length + ')';
}

function headerGet(headers, name) {
  const v = headers.get(name);
  return v === null ? undefined : v;
}

async function timedFetch(url, init = {}, timeoutMs = 30000) {
  const t0 = Date.now();
  const res = await fetch(url, { ...init, signal: AbortSignal.timeout(timeoutMs) });
  const text = await res.text();
  return { res, text, ms: Date.now() - t0 };
}

function parseFeishuBaseUrl(raw) {
  const out = { appToken: '', tableId: '', raw: raw || '' };
  if (!raw) return out;
  let u;
  try {
    u = new URL(raw);
  } catch {
    return out;
  }
  const base = u.pathname.match(/\/base\/([A-Za-z0-9]+)/);
  const wiki = u.pathname.match(/\/wiki\/([A-Za-z0-9]+)/);
  out.appToken = base ? base[1] : '';
  out.tableId = u.searchParams.get('table') || '';
  if (wiki) {
    out.wikiToken = wiki[1];
    out.type = 'wiki';
  } else if (base) {
    out.type = 'base';
  }
  return out;
}

/* -------------------------------- Jina -------------------------------- */

async function probeJina() {
  if (!cfg.jinaKey) return { status: 'skipped', reason: 'JINA_API_KEY not set' };
  const target = 'https://r.jina.ai/https://example.com';
  try {
    const { res, text, ms } = await timedFetch(
      target,
      {
        headers: {
          Authorization: 'Bearer ' + cfg.jinaKey,
          Accept: 'text/plain',
          'X-Return-Format': 'text',
        },
      },
      60000,
    );
    const rate = {
      limit: headerGet(res.headers, 'x-ratelimit-limit'),
      remaining: headerGet(res.headers, 'x-ratelimit-remaining'),
      reset: headerGet(res.headers, 'x-ratelimit-reset'),
    };
    const body = text.slice(0, 160);
    if (res.status === 200) {
      return {
        status: 'ok',
        httpStatus: 200,
        latencyMs: ms,
        bodyPreview: body.replace(/\s+/g, ' '),
        rateLimitHeaders: rate,
      };
    }
    return {
      status: 'fail',
      httpStatus: res.status,
      latencyMs: ms,
      rateLimitHeaders: rate,
      hint:
        res.status === 401
          ? 'key rejected (check it is the Reader API key, not a model API key)'
          : res.status === 402
            ? 'out of credit / plan exhausted'
            : res.status === 429
              ? 'rate limited -- lower concurrency'
              : 'unexpected HTTP ' + res.status,
      bodyPreview: body.replace(/\s+/g, ' '),
    };
  } catch (err) {
    return { status: 'fail', error: String(err && err.message ? err.message : err) };
  }
}

/* ------------------------------- YouTube ------------------------------ */

const SAMPLE_VIDEO_ID = 'jNQXAC9IVRw'; // "Me at the zoo", always public
async function probeYouTube() {
  if (!cfg.youtubeKey) return { status: 'skipped', reason: 'YOUTUBE_API_KEY not set' };
  const url = new URL('https://www.googleapis.com/youtube/v3/videos');
  url.searchParams.set('part', 'snippet');
  url.searchParams.set('id', SAMPLE_VIDEO_ID);
  url.searchParams.set('key', cfg.youtubeKey);
  try {
    const { res, text, ms } = await timedFetch(url.toString(), {}, 20000);
    let json = null;
    try {
      json = JSON.parse(text);
    } catch {
      /* keep null */
    }
    if (res.status === 200 && json && Array.isArray(json.items) && json.items.length > 0) {
      return {
        status: 'ok',
        httpStatus: 200,
        latencyMs: ms,
        sampleTitle: json.items[0].snippet && json.items[0].snippet.title,
        quotaCost: 1,
        quotaNote: 'default 10,000 units/day; videos.list = 1 unit; search.list has its own 100 calls/day bucket',
      };
    }
    const reason =
      json && json.error && Array.isArray(json.error.errors) && json.error.errors[0]
        ? json.error.errors[0].reason
        : null;
    const hintByReason = {
      keyInvalid: 'API key is not valid',
      badRequest: 'key parameter rejected',
      accessNotConfigured: 'YouTube Data API v3 is NOT enabled for this Google Cloud project',
      quotaExceeded: 'daily quota exhausted for this project',
      forbidden: 'project/key restricted (API restrictions?)',
      ipRefererBlocked: 'key restricted to other IPs/referrers',
    };
    return {
      status: 'fail',
      httpStatus: res.status,
      latencyMs: ms,
      reason,
      hint: (reason && hintByReason[reason]) || 'see error below',
      errorMessage: json && json.error ? json.error.message : text.slice(0, 300),
    };
  } catch (err) {
    return { status: 'fail', error: String(err && err.message ? err.message : err) };
  }
}

/* -------------------------------- 飞书 -------------------------------- */

const FEISHU_CODES = {
  10003: 'invalid param (app_id / app_secret shape)',
  10012: 'app_secret invalid',
  10014: 'app_id not found',
  10015: 'app not installed / not enabled',
  91403: 'Forbidden -- app has NO access to this Base. Add the app as a collaborator of the Base.',
  1254000: 'app_token invalid',
  1254002: 'table_id invalid',
  1254003: 'wrong app_token for this table',
  1254004: 'app has no permission on this table',
  1254005: 'record not found',
  1254302: 'no permission to read records: grant base:record:retrieve (or bitable:app) and republish a version',
  1254303: 'no permission to write records: grant base:record:update (or bitable:app)',
};

async function feishuJson(url, init = {}) {
  const { res, text, ms } = await timedFetch(url, init, 25000);
  let json = null;
  try {
    json = JSON.parse(text);
  } catch {
    /* keep null */
  }
  return { res, text, json, ms };
}

async function probeFeishu() {
  if (!cfg.feishuAppId || !cfg.feishuAppSecret) {
    return { status: 'skipped', reason: 'FEISHU_APP_ID / FEISHU_APP_SECRET not set' };
  }
  const out = { step: 'tenant_access_token' };
  const tokenRes = await feishuJson('https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json; charset=utf-8' },
    body: JSON.stringify({ app_id: cfg.feishuAppId, app_secret: cfg.feishuAppSecret }),
  });
  const tokenJson = tokenRes.json;
  if (!tokenJson || tokenJson.code !== 0 || !tokenJson.tenant_access_token) {
    const code = tokenJson ? tokenJson.code : 'n/a';
    return {
      status: 'fail',
      step: 'tenant_access_token',
      code,
      msg: tokenJson ? tokenJson.msg : tokenRes.text.slice(0, 200),
      hint: FEISHU_CODES[code] || 'check app_id / app_secret in the developer console',
    };
  }
  out.tokenOk = true;
  out.tokenExpireSeconds = tokenJson.expire;
  const auth = { Authorization: 'Bearer ' + tokenJson.tenant_access_token };

  const base = parseFeishuBaseUrl(cfg.feishuBaseUrl);
  out.baseUrl = base.raw || '(not set)';
  out.baseType = base.type || 'unknown';
  out.appToken = base.appToken || '';
  out.tableId = base.tableId || '';

  if (!out.appToken) {
    out.note = 'no FEISHU_BASE_URL: token OK, but Base/Table ID not verified';
    return { status: 'partial', ...out };
  }

  const tablesRes = await feishuJson(
    'https://open.feishu.cn/open-apis/bitable/v1/apps/' + out.appToken + '/tables?page_size=100',
    { headers: auth },
  );
  const tablesJson = tablesRes.json;
  if (!tablesJson || tablesJson.code !== 0) {
    const code = tablesJson ? tablesJson.code : 'n/a';
    return {
      ...out,
      status: 'fail',
      step: 'list_tables',
      code,
      msg: tablesJson ? tablesJson.msg : tablesRes.text.slice(0, 200),
      hint: FEISHU_CODES[code] || 'add the app as a collaborator of the Base',
    };
  }
  out.tables = (tablesJson.data.items || []).map((t) => ({ tableId: t.table_id, name: t.name }));

  if (!out.tableId && out.tables.length) {
    out.tableId = out.tables[0].tableId;
    out.note = 'table_id not in URL; fell back to first table (' + out.tables[0].name + ')';
  }

  let fieldNames = [];
  if (out.tableId) {
    const fieldsRes = await feishuJson(
      'https://open.feishu.cn/open-apis/bitable/v1/apps/' +
        out.appToken +
        '/tables/' +
        out.tableId +
        '/fields?page_size=100',
      { headers: auth },
    );
    if (fieldsRes.json && fieldsRes.json.code === 0) {
      fieldNames = (fieldsRes.json.data.items || []).map((f) => f.field_name);
      out.fields = fieldNames;
    } else {
      out.fieldError = {
        code: fieldsRes.json && fieldsRes.json.code,
        msg: fieldsRes.json && fieldsRes.json.msg,
      };
    }

    const searchRes = await feishuJson(
      'https://open.feishu.cn/open-apis/bitable/v1/apps/' +
        out.appToken +
        '/tables/' +
        out.tableId +
        '/records/search?page_size=5',
      {
        method: 'POST',
        headers: { ...auth, 'Content-Type': 'application/json; charset=utf-8' },
        body: JSON.stringify({}),
      },
    );
    const searchJson = searchRes.json;
    if (searchJson && searchJson.code === 0) {
      out.recordsReadable = true;
      out.sampleRecordCount = ((searchJson.data && searchJson.data.items) || []).length;
      out.totalRecords = searchJson.data && searchJson.data.total;
    } else {
      out.recordsReadable = false;
      out.searchError = { code: searchJson && searchJson.code, msg: searchJson && searchJson.msg };
      out.hint =
        FEISHU_CODES[out.searchError.code] || 'grant base:record:retrieve and republish the app version';
      return { ...out, status: 'partial' };
    }
  }

  return { status: 'ok', ...out, step: 'search_records' };
}

/* -------------------------------- main -------------------------------- */

const only = args.only ? args.only.toLowerCase() : null;
const run = (name) => !only || only === name;

log('AetherHUD credential check -- ' + startedAt);
log('');
log('  Jina key     : ' + mask(cfg.jinaKey));
log('  YouTube key  : ' + mask(cfg.youtubeKey));
log('  Feishu app_id: ' + mask(cfg.feishuAppId));
log('  Feishu secret: ' + mask(cfg.feishuAppSecret));
log('  Feishu URL   : ' + (cfg.feishuBaseUrl || '(not set)'));
log('');

if (run('jina')) {
  log('[jina] probing r.jina.ai ...');
  results.jina = await probeJina();
  log('  -> ' + results.jina.status + (results.jina.httpStatus ? ' HTTP ' + results.jina.httpStatus : '') +
      (results.jina.latencyMs ? ' ' + results.jina.latencyMs + 'ms' : ''));
  if (results.jina.hint) log('     hint: ' + results.jina.hint);
  if (results.jina.rateLimitHeaders) log('     ratelimit: ' + JSON.stringify(results.jina.rateLimitHeaders));
}

if (run('youtube')) {
  log('[youtube] probing videos.list ...');
  results.youtube = await probeYouTube();
  log('  -> ' + results.youtube.status + (results.youtube.httpStatus ? ' HTTP ' + results.youtube.httpStatus : '') +
      (results.youtube.latencyMs ? ' ' + results.youtube.latencyMs + 'ms' : ''));
  if (results.youtube.sampleTitle) log('     sample: ' + results.youtube.sampleTitle);
  if (results.youtube.hint) log('     hint: ' + results.youtube.hint);
}

if (run('feishu')) {
  log('[feishu] probing tenant_access_token -> tables -> fields -> records ...');
  results.feishu = await probeFeishu();
  log('  -> ' + results.feishu.status);
  if (results.feishu.tokenOk) log('     tenant_access_token OK (expires in ' + results.feishu.tokenExpireSeconds + 's)');
  if (results.feishu.appToken) log('     app_token: ' + results.feishu.appToken);
  if (results.feishu.tableId) log('     table_id : ' + results.feishu.tableId);
  if (results.feishu.tables) {
    log('     tables (' + results.feishu.tables.length + '):');
    for (const t of results.feishu.tables) log('       - ' + t.tableId + '  ' + t.name);
  }
  if (results.feishu.fields) log('     fields (' + results.feishu.fields.length + '): ' + results.feishu.fields.join(', '));
  if (results.feishu.totalRecords !== undefined) {
    log('     records read: sample=' + results.feishu.sampleRecordCount + ' total=' + results.feishu.totalRecords);
  }
  if (results.feishu.hint) log('     hint: ' + results.feishu.hint);
  if (results.feishu.msg) log('     msg: ' + results.feishu.msg + ' (code ' + results.feishu.code + ')');
}

const report = {
  generatedAt: new Date().toISOString(),
  envFile: envPath,
  results,
  summary: {
    jina: results.jina && results.jina.status,
    youtube: results.youtube && results.youtube.status,
    feishu: results.feishu && results.feishu.status,
  },
};

mkdirSync(dirname(REPORT_PATH), { recursive: true });
writeFileSync(REPORT_PATH, JSON.stringify(report, null, 2), 'utf8');

log('');
log('summary: ' + JSON.stringify(report.summary));
log('report : ' + REPORT_PATH);

const bad = Object.values(report.summary).some((s) => s === 'fail');
process.exit(bad ? 1 : 0);
