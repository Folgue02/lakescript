#!/usr/bin/env python3
"""Builds the javadoc-style page of the standard library from the notes in
ideas/stdlib: the README, the types in types/*.md and the modules in *.md.

    python3 tools/docs/stdlib.py                 # writes target/docs/stdlib.html
    python3 tools/docs/stdlib.py out.html        # writes out.html
"""
import html, json, re, sys
from pathlib import Path

here = Path(__file__).resolve().parent
repo = here.parent.parent
root = repo / 'ideas' / 'stdlib'
out = Path(sys.argv[1]) if len(sys.argv) > 1 else repo / 'target' / 'docs' / 'stdlib.html'
TYPES = ['any', 'string', 'number', 'bool', 'list', 'map', 'range', 'symbol', 'regex', 'command']
MODULES = ['general', 'fs', 'path', 'env', 'io', 'process', 'json']
PAGES = set(TYPES) | set(MODULES)
MEMBER_HEADERS = ('Method', 'Function', 'Constant')


def inline(text):
    text = text.replace('\\|', '\x00')
    parts = re.split(r'(`[^`]*`)', text)
    res = []
    for p in parts:
        if p.startswith('`') and p.endswith('`') and len(p) > 1:
            res.append('<code>' + html.escape(p[1:-1]).replace('\x00', '|') + '</code>')
        else:
            s = html.escape(p).replace('\x00', '|')

            def link(m):
                label, href = m.group(1), m.group(2)
                name = Path(href.split('#')[0]).stem
                if name in PAGES:
                    return f'<a href="#{name}">{label}</a>'
                return f'<span class="ref">{label}</span>'

            s = re.sub(r'\[([^\]]+)\]\(([^)]+)\)', link, s)
            s = re.sub(r'\*\*([^*]+)\*\*', r'<strong>\1</strong>', s)
            s = re.sub(r'\*([^*]+)\*', r'<em>\1</em>', s)
            res.append(s)
    return ''.join(res)


def cells(line):
    line = line.strip().replace('\\|', '\x01')
    return [c.strip().replace('\x01', '\\|') for c in line.strip('|').split('|')]


def md_block(text):
    """A small markdown renderer: headings, bullets, paragraphs, tables and code."""
    out, lines, i = [], text.splitlines(), 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        if stripped.startswith('```'):
            code = []
            i += 1
            while i < len(lines) and not lines[i].strip().startswith('```'):
                code.append(lines[i])
                i += 1
            out.append(f'<pre class="example"><code>{html.escape(chr(10).join(code))}</code></pre>')
        elif line.startswith('#'):
            level = len(line) - len(line.lstrip('#'))
            out.append(f'<h{level + 1}>{inline(line.lstrip("# "))}</h{level + 1}>')
        elif stripped.startswith('- '):
            items = []
            while i < len(lines) and lines[i].strip().startswith('- '):
                nested = ' class="nested"' if lines[i].startswith(' ') else ''
                items.append(f'<li{nested}>{inline(lines[i].strip()[2:])}</li>')
                i += 1
            out.append('<ul>' + ''.join(items) + '</ul>')
            continue
        elif stripped.startswith('|'):
            rows = []
            while i < len(lines) and lines[i].strip().startswith('|'):
                rows.append(cells(lines[i]))
                i += 1
            head, body = rows[0], rows[2:]
            t = '<div class="scroll"><table class="summary"><thead><tr>' + ''.join(f'<th>{inline(h)}</th>' for h in head) + '</tr></thead><tbody>'
            t += ''.join('<tr>' + ''.join(f'<td>{inline(c)}</td>' for c in r) + '</tr>' for r in body)
            out.append(t + '</tbody></table></div>')
            continue
        elif stripped:
            out.append(f'<p>{inline(stripped)}</p>')
        i += 1
    return '\n'.join(out)


def parse_doc(path):
    """The title, the intro (markdown), and the sections with their members
    (from the Method/Function/Constant tables) and their other text."""
    lines = path.read_text().splitlines()
    title = lines[0].lstrip('# ').strip()
    intro, sections, cur = [], [], None
    i = 1
    while i < len(lines):
        line = lines[i]
        if line.startswith('## '):
            cur = {'name': line[3:].strip(), 'text': [], 'rows': []}
            sections.append(cur)
        elif line.strip().startswith('|') and cells(line)[0] in MEMBER_HEADERS and cur is not None:
            i += 2  # the header and the `---` line
            while i < len(lines) and lines[i].strip().startswith('|'):
                cur['rows'].append(cells(lines[i])[:3])
                i += 1
            continue
        else:
            (cur['text'] if cur else intro).append(line)
        i += 1
    return title, '\n'.join(intro), sections


def sig(member):
    m = re.match(r'([\w?$]+)\((.*)\)$', member)
    if not m:
        return member, f'<span class="mname">{html.escape(member)}</span>'
    name, params = m.group(1), m.group(2)
    p = html.escape(params.replace('\\|', '|'))
    p = re.sub(r'(\$\w+)', r'<span class="param">\1</span>', p)
    return name, f'<span class="mname">{html.escape(name)}</span>(<span class="params">{p}</span>)'


docs = []
for name in TYPES:
    docs.append((name, 'type', *parse_doc(root / 'types' / f'{name}.md')))
for name in MODULES:
    docs.append((name, 'module', *parse_doc(root / f'{name}.md')))

readme = (root / 'README.md').read_text().split('\n', 1)[1]
overview = md_block(readme)


def nav_items(kind):
    return ''.join(
        f'<li><a href="#{n}" data-nav="{n}"><code>{html.escape(t)}</code></a></li>'
        for n, k, t, _, _ in docs if k == kind
    )


pages = []
index = []
for name, kind, title, intro, sections in docs:
    members = [s for s in sections if s['rows']]
    count = sum(len(s['rows']) for s in sections)
    noun = 'methods' if kind == 'type' else 'functions'
    tabs = f'<button class="tab active" data-cat="all" type="button">All {noun}</button>' + ''.join(
        f'<button class="tab" data-cat="{i}" type="button">{html.escape(s["name"])}</button>' for i, s in enumerate(members))
    rows, details, notes = [], [], []
    for i, s in enumerate(members):
        text = md_block('\n'.join(s['text']))
        note = f'<div class="note">{text}</div>' if text else ''
        rows.append(f'<tr class="group" data-cat="{i}"><th colspan="2">{html.escape(s["name"])}{note}</th></tr>')
        for member, returns, desc in s['rows']:
            mname, signature = sig(member.strip('`'))
            anchor = re.sub(r'[^A-Za-z0-9-]', '', f'{name}-{mname}'.replace('?', 'Q'))
            first = desc.split(' *(')[0]
            shown = f'{name}::{mname}' if kind == 'module' and name != 'general' else mname
            rows.append(
                f'<tr data-cat="{i}">'
                f'<td class="ret"><code>{inline(returns)}</code></td>'
                f'<td><code class="sig"><a href="#{anchor}">{signature}</a></code><div class="desc">{inline(first)}</div></td></tr>')
            details.append(
                f'<section class="detail" id="{anchor}"><h4>{html.escape(shown)}</h4>'
                f'<pre class="decl"><code>{html.escape(returns.strip("`").replace(chr(92) + "|", "|"))} {signature}</code></pre>'
                f'<div class="block">{inline(desc)}</div><p class="cat">Group: {html.escape(s["name"])}</p></section>')
            index.append({'t': title, 'm': shown, 'a': anchor, 'd': re.sub('<[^>]+>', '', inline(first))})
    # Sections without members (mapping tables, notes) go after the summary
    for s in sections:
        if not s['rows'] and s['text']:
            notes.append(f'<h2 class="bar">{html.escape(s["name"])}</h2><div class="prose">{md_block(chr(10).join(s["text"]))}</div>')
    heading = f'Type <code>{html.escape(title)}</code>' if kind == 'type' else f'Module <code>{html.escape(title)}</code>'
    if name == 'general':
        heading = 'Global functions'
    crumb = 'lakescript.types' if kind == 'type' else 'lakescript.std'
    summary = ''
    if count:
        label = 'Method' if kind == 'type' else 'Function'
        summary = f'''
  <h2 class="bar">{label} Summary <span class="count">{count} {noun}</span></h2>
  <div class="tabs" role="tablist">{tabs}</div>
  <div class="scroll"><table class="summary methods">
    <thead><tr><th>Returns</th><th>{label} and description</th></tr></thead>
    <tbody>{''.join(rows)}</tbody>
  </table></div>'''
    detail = f'<h2 class="bar">{label} Detail</h2>{"".join(details)}' if count else ''
    pages.append(f'''
<article class="page" data-page="{name}" hidden>
  <div class="crumb">{crumb}</div>
  <h1>{heading}</h1>
  <div class="intro">{md_block(intro)}</div>{summary}
  {''.join(notes)}
  {detail}
</article>''')

template = (here / 'stdlib-template.html').read_text()
page = (template.replace('{{NAV}}', nav_items('type'))
        .replace('{{NAV_MODULES}}', nav_items('module'))
        .replace('{{OVERVIEW}}', overview)
        .replace('{{PAGES}}', ''.join(pages))
        .replace('{{INDEX}}', json.dumps(index))
        .replace('{{TOTAL}}', str(len(index))))
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(page)
print(f'{out}: {len(index)} entries')
