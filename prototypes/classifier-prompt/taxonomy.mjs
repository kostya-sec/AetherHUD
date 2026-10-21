
import fs from 'node:fs';

export const TAXONOMY = JSON.parse(fs.readFileSync('taxonomy.json', 'utf8'));
export const FORMAT_TAGS = TAXONOMY.global_l3_tags.format_tags;

export const L1S = TAXONOMY.taxonomy.map(l1 => ({
  id: l1.id,
  name: l1.name,
  children: l1.children.map(c => ({ id: c.id, name: c.name, presets: c.l3_preset_tags || [] })),
}));

export const NORMAL_L1 = L1S.filter(l1 => l1.id !== 'l1_inbox');
export const NORMAL_L2 = NORMAL_L1.flatMap(l1 => l1.children.map(c => ({ ...c, parent: l1.id })));
export const ALL_L2 = L1S.flatMap(l1 => l1.children.map(c => ({ ...c, parent: l1.id })));
export const L2_PARENT = Object.fromEntries(ALL_L2.map(c => [c.id, c.parent]));
export const ALL_L1_IDS = new Set(L1S.map(l => l.id));
export const NORMAL_L1_IDS = new Set(NORMAL_L1.map(l => l.id));
export const NORMAL_L2_IDS = new Set(NORMAL_L2.map(l => l.id));
export const ALL_L2_IDS = new Set(ALL_L2.map(l => l.id));
export const FORMAT_TAG_IDS = new Set(FORMAT_TAGS.map(t => t.id));

export function renderTree() {
  const lines = [];
  for (const l1 of L1S) {
    lines.push(l1.id + ' ' + l1.name);
    for (const c of l1.children) {
      const preset = c.presets.length ? '｜预设标签：' + c.presets.join('、') : '';
      lines.push('  · ' + c.name + '（' + c.id + '）' + preset);
    }
  }
  return lines.join('\n');
}
