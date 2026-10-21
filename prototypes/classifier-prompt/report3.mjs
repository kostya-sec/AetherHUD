
import fs from 'node:fs';
const rd = p => fs.readFileSync(p, 'utf8').split(/\r?\n/).filter(Boolean).map(l => JSON.parse(l));
const samples = JSON.parse(fs.readFileSync('.local/proto/cp/data/samples.json', 'utf8'));
const gold = Object.fromEntries(samples.map(s => [s.key, s.gold]));
const rows = rd('.local/proto/cp/data/results-samples.jsonl');
const variants = ['A', 'B', 'C', 'D', 'C2', 'C3', 'C4'];

function microF1(pairs) {
  let tp = 0, fp = 0, fn = 0;
  for (const { out, gold } of pairs) {
    const o = new Set(out), g = new Set(gold);
    for (const t of o) (g.has(t) ? tp++ : fp++);
    for (const t of g) if (!o.has(t)) fn++;
  }
  const p = tp + fp ? tp / (tp + fp) : 0;
  const r = tp + fn ? tp / (tp + fn) : 0;
  return { f1: p + r ? (2 * p * r) / (p + r) : 0, precision: p, recall: r };
}

const out = [];
for (const v of variants) {
  const rs = rows.filter(r => r.variant === v);
  if (!rs.length) continue;
  const ok = rs.filter(r => r.final.status === 'ok');
  const firstPass = rs.filter(r => r.attempts[0]?.ok).length;
  const l1 = ok.filter(r => r.final.parsed.l1_id === gold[r.key].l1_id).length;
  const l2 = ok.filter(r => r.final.parsed.l2_id === gold[r.key].l2_id).length;
  const ft = microF1(ok.map(r => ({ out: r.final.parsed.format_tags, gold: gold[r.key].format_tags })));
  const calls = rs.reduce((a, r) => a + r.attempts.length, 0);
  const promptTok = rs.reduce((a, r) => a + r.attempts.reduce((x, y) => x + (y.usage?.prompt_tokens || 0), 0), 0);
  const compTok = rs.reduce((a, r) => a + r.attempts.reduce((x, y) => x + (y.usage?.completion_tokens || 0), 0), 0);
  const ms = rs.reduce((a, r) => a + r.attempts.reduce((x, y) => x + (y.ms || 0), 0), 0);
  out.push({
    '版本': v,
    '首轮合规': firstPass + '/' + rs.length,
    '有效': ok.length + '/' + rs.length,
    '弃权': ok.filter(r => r.final.abstained).length,
    'L1准确': l1 + '/' + ok.length,
    'L2准确': l2 + '/' + ok.length,
    '形态F1': ft.f1.toFixed(2),
    '形态P': ft.precision.toFixed(2),
    '形态R': ft.recall.toFixed(2),
    '重试': calls - rs.length,
    '平均输入tok': Math.round(promptTok / rs.length),
    '平均输出tok': Math.round(compTok / rs.length),
    '平均耗时ms': Math.round(ms / rs.length),
  });
}
console.log(out.map(o => Object.values(o).join(' | ')).join('\n'));
console.log('\nHEADER:', Object.keys(out[0]).join(' | '));
