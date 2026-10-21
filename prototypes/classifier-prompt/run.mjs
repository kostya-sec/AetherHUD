
import fs from 'node:fs';
import { buildMessages, repairMessages, MODEL, VARIANTS } from './prompt.mjs';
import { validate } from './validate.mjs';

const raw = fs.readFileSync('.local/keys.env', 'utf8');
const env = {};
for (const l of raw.split(/\r?\n/)) { const m = l.match(/^([A-Z0-9_]+)=(.*)$/); if (m) env[m[1]] = m[2].trim(); }
const KEY = env.DASHSCOPE_API_KEY;
const URL = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions';

const setFile = process.env.SET || 'samples';
const samples = JSON.parse(fs.readFileSync('.local/proto/cp/data/' + setFile + '.json', 'utf8'));
const variants = (process.env.VARIANTS || 'A,B,C,D').split(',');
const outPath = '.local/proto/cp/data/results-' + setFile + '.jsonl';
if (process.env.FRESH === '1') fs.writeFileSync(outPath, '');

const sleep = ms => new Promise(r => setTimeout(r, ms));

async function call(messages) {
  for (let attempt = 1; attempt <= 3; attempt++) {
    const t0 = Date.now();
    try {
      const res = await fetch(URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + KEY },
        body: JSON.stringify({
          model: MODEL,
          messages,
          temperature: 0,
          response_format: { type: 'json_object' },
          max_tokens: 400,
          enable_thinking: false,
        }),
      });
      const ms = Date.now() - t0;
      const j = await res.json();
      if (res.status === 200) {
        return {
          ok: true,
          content: j.choices?.[0]?.message?.content ?? '',
          usage: j.usage || {},
          ms,
          model: j.model,
        };
      }
      const retryable = res.status === 429 || res.status >= 500;
      if (!retryable) return { ok: false, error: 'HTTP ' + res.status + ' ' + JSON.stringify(j).slice(0, 300), ms };
      await sleep(1000 * attempt);
    } catch (e) {
      if (attempt === 3) return { ok: false, error: 'NET ' + e.message, ms: Date.now() - t0 };
      await sleep(1000 * attempt);
    }
  }
  return { ok: false, error: 'unreachable' };
}

const records = [];
for (const variant of variants) {
  const v = VARIANTS[variant];
  for (const sample of samples) {
    const messages = buildMessages(variant, sample);
    const first = await call(messages);
    const row = {
      variant, key: sample.key, model: MODEL, prompt_label: v.label,
      attempts: [], final: null,
    };
    if (!first.ok) {
      row.attempts.push({ attempt: 1, ok: false, error: first.error, ms: first.ms });
      row.final = { status: 'unavailable', error: first.error };
      records.push(row);
      fs.appendFileSync(outPath, JSON.stringify(row) + '\n');
      console.log(variant, sample.key, 'UNAVAILABLE', first.error);
      continue;
    }
    let cur = first;
    let valid = null;
    for (let attempt = 1; attempt <= 1 + v.retry; attempt++) {
      const parsed = safeParse(cur.content);
      const val = validate(parsed);
      row.attempts.push({
        attempt, raw: cur.content, parsed: parsed, ok: val.ok, errors: val.errors,
        normalized: val.normalized, ms: cur.ms, usage: cur.usage,
      });
      valid = val;
      if (val.ok || attempt === 1 + v.retry) break;
      const next = repairMessages(attempt === 1 ? messages : messages, cur.content, val.errors);
      const repaired = await call(next);
      if (!repaired.ok) { row.final = { status: 'unavailable', error: repaired.error }; break; }
      cur = repaired;
    }
    if (!row.final) {
      row.final = valid.ok
        ? { status: 'ok', parsed: valid.normalized, abstained: valid.abstained, suggestion_errors: valid.normalized?.suggestion_errors || null }
        : { status: 'invalid', errors: valid.errors, parsed: valid.normalized };
    }
    records.push(row);
    fs.appendFileSync(outPath, JSON.stringify(row) + '\n');
    const usedPrompt = row.attempts.reduce((a, x) => a + (x.usage?.prompt_tokens || 0), 0);
    const usedCompletion = row.attempts.reduce((a, x) => a + (x.usage?.completion_tokens || 0), 0);
    console.log(variant, sample.key, row.final.status, 'tokens', usedPrompt + '/' + usedCompletion, 'ms', row.attempts.reduce((a, x) => a + (x.ms || 0), 0),
      row.final.status === 'ok' ? row.final.parsed.l1_id + '>' + row.final.parsed.l2_id + ' tags:' + row.final.parsed.format_tags.join('+') + ' feat:' + (row.final.parsed.feature_tags || []).join(',') : JSON.stringify(row.final).slice(0, 200));
  }
}
console.log('DONE', records.length, 'rows ->', outPath);

function safeParse(s) {
  try { return JSON.parse(s); } catch { return null; }
}
