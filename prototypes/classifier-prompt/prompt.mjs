
import fs from 'node:fs';
import { renderTree, L1S } from './taxonomy.mjs';

const FEWSHOT = JSON.parse(fs.readFileSync('.local/proto/cp/data/fewshot.json', 'utf8'));
const POOL = [
  'Claude Code', 'Codex', 'OpenClaw', 'Skill', 'GitHub', 'MCP', 'Seedance', 'Gemini',
  'Anthropic', 'OpenAI', 'Obsidian', 'Rust', 'ChatGPT', '飞书', 'Docker', 'Karpathy',
  'Suno', '分镜', '即梦', 'MiniMax', 'Prompt123', 'Coze', 'Dify', 'ComfyUI', 'Midjourney', 'Notion',
];

export const MODEL = process.env.PROTO_MODEL || 'qwen-flash';

function renderTreeOrdered(inboxLast) {
  if (!inboxLast) return renderTree();
  const order = [...L1S.filter(l => l.id !== 'l1_inbox'), ...L1S.filter(l => l.id === 'l1_inbox')];
  const lines = [];
  for (const l1 of order) {
    const note = l1.id === 'l1_inbox' ? '（仅用于弃权）' : '';
    lines.push(l1.id + ' ' + l1.name + note);
    for (const c of l1.children) {
      const preset = c.presets.length ? '｜预设标签：' + c.presets.join('、') : '';
      lines.push('  · ' + c.name + '（' + c.id + '）' + preset);
    }
  }
  return lines.join('\n');
}

const ROLE = '你是 AetherHUD 的内容分类器。用户会给你一条自己收藏的内容（标题、链接、媒体类型、正文摘录），你要把它归入固定分类树中的一个 L2，并给出筛选标签。';

const TAG_RULES = [
  '【标签】',
  '- format_tags（形态标签）：从固定 5 个 id 中选 1~2 个',
  '  tag_tool=工具、tag_tutorial=教程、tag_prompt=提示词、tag_workflow=工作流、tag_asset=素材/资源',
  '- feature_tags（特征标签）：0~3 个，取原文中出现的专有名词 / 产品名 / 领域词（如 Cursor、Obsidian、Seedance），每个 1~20 字；不要泛词（AI、工具、大模型）。',
].join('\n');

const TAG_RULES_TIGHT = [
  '【标签】',
  '- format_tags（形态标签）：从固定 5 个 id 中选 1~2 个；多数内容 1 个就够，只有确实同时呈现两种形态（如「工具 + 教程」）才给 2 个。',
  '  tag_tool=工具、tag_tutorial=教程、tag_prompt=提示词、tag_workflow=工作流、tag_asset=素材/资源',
  '- feature_tags（特征标签）：最多 3 个，宁少不多；只取原文里最关键、用户真会用来搜索的专有名词，每个 1~20 字；不要泛词（AI、工具、大模型）。',
].join('\n');

const JUDGE_RULES = [
  '【判定方法】',
  '1. 先判断 L1（内容与表达 / 效率与工作流 / 技术与工程 / 商业与认知），再在该 L1 下选最合适的 L2。',
  '2. 以「用户以后会去哪里找它」为准；跨类时按主要用途归类。',
  '3. 媒体类型不是分类依据：视频、文章只是载体，讲编程的视频归 l2_coding，不因是视频就归「视频与音频」。',
  '4. 标题很短、只有链接或信息不全时，从正文摘录和链接域名判断意图。',
].join('\n');

const POOL_BLOCK = [
  '【特征标签候选池】（优先复用，写法保持一致；确有更准确的新词才新增）',
  POOL.join('、'),
].join('\n');

const ABSTAIN_RULES = [
  '【何时弃权】',
  '只有内容确实无法归入 12 个正常 L2 中任何一个时才弃权：输出 l1_inbox + l2_manual_review。',
  '- 只是不确定、跨类、标题短、信息少，仍然要选最可能的一个 L2，不要弃权；先看正文再看标题。',
  '- 弃权只用于「确实没有合适类别」；弃权率过高会加重人工负担（目标 ≤5%）。',
].join('\n');

const ABSTAIN_RULES_V2 = [
  '【何时弃权】',
  '只有两种情况允许弃权（输出 l1_inbox + l2_manual_review）：',
  '1. 几乎没有可判断的信息：只有一句噪声、一个域名或裸链接，看不出主题；',
  '2. 主题清楚，但确实不属于分类树里的任何一个正常 L2。',
  '其余情况（不确定、跨类、标题短但有正文、在两个 L2 之间犹豫）都要选最可能的一个，不要弃权；弃权率目标 ≤5%。',
].join('\n');

const UNCOVERED_NOTE = '- 已知不在覆盖范围的主题例子：通识学习（教材/课程/科普）、生活健康（健身/养生/医疗）、时事新闻（纯资讯）、纯娱乐（段子/追星）。';

const SUGGEST_RULES = [
  '【弃权时给建议】',
  '弃权时额外输出 suggestion，帮助人快速归置；非弃权时 suggestion 固定为 null。',
  '{"nearest_l2_id":"最接近的正常 L2 id，确实无关联时为 null","new_l2":{"parent_l1_id":"4 个正常 L1 之一","name":"2~10 字的分类名"} 或 null}',
  '- new_l2 只在确有类别缺口时给；名字要短、具体、可复用（如「短视频脚本」「交易与投资」）。',
  '- 建议只是线索、不参与评分；不要为了能填建议而弃权。',
].join('\n');

const CONTRACT_BASE = [
  '【输出】只输出一个 JSON 对象，不要解释、不要 Markdown 代码块。',
  '{"l1_id":"l1_xxx","l2_id":"l2_xxx","format_tags":["tag_xxx"],"feature_tags":["xxx"]}',
].join('\n');

const CONTRACT_SUGGEST = [
  '【输出】只输出一个 JSON 对象，不要解释、不要 Markdown 代码块。',
  '{"l1_id":"l1_xxx","l2_id":"l2_xxx","format_tags":["tag_xxx"],"feature_tags":["xxx"],"suggestion":null}',
].join('\n');

const SELF_CHECK_SUGGEST = [
  '【输出前自检】',
  '- l1_id / l2_id 都在分类树中，且 l2_id 属于该 l1_id；',
  '- format_tags 为 1~2 个合法 id；feature_tags 不超过 3 个、每个 1~20 字；',
  '- 弃权时 suggestion 非 null 且字段合法；非弃权时 suggestion 为 null。',
].join('\n');

const SELF_CHECK_PLAIN = [
  '【输出前自检】',
  '- l1_id / l2_id 都在分类树中，且 l2_id 属于该 l1_id；',
  '- format_tags 为 1~2 个合法 id；feature_tags 不超过 3 个、每个 1~20 字。',
].join('\n');

const FEWSHOT_INTRO = '【示例】下面是已核对过的分类示例，注意判定口径与 JSON 形态。';

export const VARIANTS = {
  A: { label: 'V-A 朴素基线：全量树 + JSON 契约，无 few-shot / 无候选池 / 无弃权指令', fewshot: 0, judge: false, pool: false, abstain: false, suggest: false, retry: 0 },
  B: { label: 'V-B A + 判定方法 + 特征标签候选池 + 6 条 few-shot', fewshot: 6, judge: true, pool: true, abstain: false, suggest: false, retry: 0 },
  C: { label: 'V-C B + 弃权/建议指令 + 2 条弃权示例 + 校验重试', fewshot: 8, judge: true, pool: true, abstain: true, suggest: true, retry: 1 },
  D: { label: 'V-D 同 C 但去掉 suggestion 字段（对照）', fewshot: 8, judge: true, pool: true, abstain: true, suggest: false, retry: 1 },
  C2: { label: 'V-C2 同 C，但收紧标签条数措辞（宁少不多 + 形态通常 1 个）', fewshot: 8, judge: true, pool: true, abstain: true, suggest: true, retry: 1, tight: true },
  C3: { label: 'V-C3 同 C2，但把 00 未分类挪到分类树末尾（标「仅用于弃权」）', fewshot: 8, judge: true, pool: true, abstain: true, suggest: true, retry: 1, tight: true, inboxLast: true, abstainV2: true },
  C4: { label: 'V-C4 同 C3，但弃权指令点名「不覆盖主题」示例', fewshot: 8, judge: true, pool: true, abstain: true, suggest: true, retry: 1, tight: true, inboxLast: true, abstainV2: true, uncovered: true },
};

export function buildSystem(variant) {
  const v = VARIANTS[variant];
  const tree = '【分类树】（只能从这些节点中选，禁止发明分类）\n' + renderTreeOrdered(v.inboxLast);
  const parts = [ROLE, tree];
  if (v.judge) parts.push(JUDGE_RULES);
  parts.push(v.tight ? TAG_RULES_TIGHT : TAG_RULES);
  if (v.pool) parts.push(POOL_BLOCK);
  if (v.abstain) parts.push(v.abstainV2 ? ABSTAIN_RULES_V2 : ABSTAIN_RULES);
  if (v.uncovered) parts.push(UNCOVERED_NOTE);
  if (v.suggest) parts.push(SUGGEST_RULES);
  parts.push(v.suggest ? CONTRACT_SUGGEST : CONTRACT_BASE);
  if (v.abstain) parts.push(v.suggest ? SELF_CHECK_SUGGEST : SELF_CHECK_PLAIN);
  return parts.join('\n\n');
}

function userContent(sample) {
  const lines = ['【待分类条目】', '标题：' + sample.title];
  if (sample.url) lines.push('链接：' + sample.url);
  const map = { link: '网页链接', video: '视频', article: '文章' };
  lines.push('媒体类型：' + (map[sample.media_type] || sample.media_type));
  lines.push('正文摘录：', sample.body_excerpt || '（无正文）');
  return lines.join('\n');
}

export function fewshotPair(ex, variant) {
  const out = { ...ex.output };
  if (!VARIANTS[variant].suggest) delete out.suggestion;
  return [
    { role: 'user', content: userContent(ex.input) },
    { role: 'assistant', content: JSON.stringify(out) },
  ];
}

export function buildMessages(variant, sample) {
  const v = VARIANTS[variant];
  const messages = [{ role: 'system', content: buildSystem(variant) }];
  if (v.fewshot > 0) {
    messages.push({ role: 'user', content: FEWSHOT_INTRO }, { role: 'assistant', content: '好的，请给待分类条目。' });
    for (const ex of FEWSHOT.slice(0, v.fewshot)) messages.push(...fewshotPair(ex, variant));
  }
  messages.push({ role: 'user', content: userContent(sample) });
  return messages;
}

export function repairMessages(messages, raw, errors) {
  return [
    ...messages,
    { role: 'assistant', content: raw },
    {
      role: 'user',
      content: '上面的输出未通过校验：\n' + errors.map(e => '- ' + e).join('\n') + '\n请只输出修正后的 JSON 对象，不要解释。',
    },
  ];
}
