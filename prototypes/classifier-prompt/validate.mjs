
import { ALL_L1_IDS, ALL_L2_IDS, L2_PARENT, NORMAL_L1_IDS, NORMAL_L2_IDS, FORMAT_TAG_IDS } from './taxonomy.mjs';

// returns { ok, errors:[], normalized }
export function validate(raw) {
  const errors = [];
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    return { ok: false, errors: ['输出不是 JSON 对象'], normalized: null };
  }
  const out = {
    l1_id: raw.l1_id,
    l2_id: raw.l2_id,
    format_tags: Array.isArray(raw.format_tags) ? raw.format_tags : [],
    feature_tags: Array.isArray(raw.feature_tags) ? raw.feature_tags : [],
    suggestion: raw.suggestion ?? null,
  };

  if (!ALL_L1_IDS.has(out.l1_id)) errors.push('l1_id 不在分类树中：' + String(out.l1_id));
  if (!ALL_L2_IDS.has(out.l2_id)) errors.push('l2_id 不在分类树中：' + String(out.l2_id));
  if (ALL_L1_IDS.has(out.l1_id) && ALL_L2_IDS.has(out.l2_id) && L2_PARENT[out.l2_id] !== out.l1_id) {
    errors.push('l2_id ' + out.l2_id + ' 不属于 ' + out.l1_id);
  }

  const ft = out.format_tags.map(t => String(t).trim()).filter(Boolean);
  for (const t of ft) if (!FORMAT_TAG_IDS.has(t)) errors.push('format_tag 非法：' + t);
  if (ft.length < 1 || ft.length > 2) errors.push('format_tags 必须 1~2 个，实际 ' + ft.length);
  out.format_tags = [...new Set(ft)];

  const fts = out.feature_tags.map(t => String(t).trim()).filter(Boolean);
  for (const t of fts) {
    if (t.length < 1 || t.length > 20) errors.push('feature_tag 长度须 1~20 字：' + t);
    if (/[\r\n\t]/.test(t)) errors.push('feature_tag 含控制字符：' + t);
  }
  if (fts.length > 3) errors.push('feature_tags 最多 3 个，实际 ' + fts.length);
  out.feature_tags = [...new Set(fts)];

  // suggestion rules (#18 §2.2)
  const abstained = out.l1_id === 'l1_inbox' && out.l2_id === 'l2_manual_review';
  if (out.l1_id === 'l1_inbox' && out.l2_id !== 'l2_manual_review') errors.push('弃权必须输出 l1_inbox + l2_manual_review');
  if (out.l2_id === 'l2_manual_review' && out.l1_id !== 'l1_inbox') errors.push('l2_manual_review 只能配 l1_inbox');
  if (!abstained && out.suggestion != null) {
    // #18: 非弃权结果带建议 = 格式错误，但忽略字段本身，不判整条失败
    out.suggestion = null;
    out.ignored_suggestion = true;
  }
  if (out.suggestion != null) {
    const s = out.suggestion;
    if (typeof s !== 'object' || Array.isArray(s)) errors.push('suggestion 必须是对象或 null');
    else {
      const nearest = s.nearest_l2_id ?? null;
      if (nearest !== null && (!NORMAL_L2_IDS.has(nearest))) {
        s.nearest_l2_id = null;
        out.suggestion_errors = [...(out.suggestion_errors || []), 'nearest_l2_id 非正常 L2，已置空'];
      }
      const nl = s.new_l2 ?? null;
      if (nl !== null) {
        const badParent = !NORMAL_L1_IDS.has(nl.parent_l1_id);
        const name = typeof nl.name === 'string' ? nl.name.trim() : '';
        const badName = name.length < 2 || name.length > 10 || /[\r\n\t\/\\]/.test(name);
        if (badParent || badName) {
          out.suggestion.new_l2 = null;
          out.suggestion_errors = [...(out.suggestion_errors || []), 'new_l2 字段非法，已丢弃'];
        } else {
          out.suggestion.new_l2 = { parent_l1_id: nl.parent_l1_id, name };
        }
      }
      const keep = out.suggestion.nearest_l2_id || out.suggestion.new_l2;
      if (!keep) out.suggestion = null;
    }
  }

  return { ok: errors.length === 0, errors, normalized: out, abstained };
}
