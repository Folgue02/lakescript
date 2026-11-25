#!/usr/bin/env python3
"""Reads the notes of the standard library in ideas/stdlib (the README, the
types in types/*.md, and the modules in *.md and format/*.md) into the data
`ferry doc std` makes its pages from, with the same design as the pages of a
project *(see ideas/tooling/docs.md)*.

    python3 tools/docs/stdlib.py                 # writes crates/ferry/src/stdlib.toml
    python3 tools/docs/stdlib.py out.toml        # writes out.toml

The data is committed in crates/ferry/src/, and ferry has it inside. Run this
after changing the notes of the standard library: a test of ferry says when
the data is out of date.

In a module, a section with a Function or Constant table gives functions or
constants, and a section with a Field or Method table gives a class *(i.e.
`Response` in http.md)*. Sections only for the people who make Lakescript
*(i.e. "Backlog")* are left out.
"""
import json, re, sys
from pathlib import Path

here = Path(__file__).resolve().parent
repo = here.parent.parent
root = repo / 'ideas' / 'stdlib'
out = Path(sys.argv[1]) if len(sys.argv) > 1 else repo / 'crates' / 'ferry' / 'src' / 'stdlib.toml'
TYPES = ['any', 'string', 'number', 'bool', 'list', 'map', 'range', 'regex', 'bytes', 'command']
# The file of each module
MODULES = ['archive', 'bytes', 'dirs', 'fs', 'path', 'env', 'io', 'process', 'random', 'resources', 'http', 'html', 'format/json', 'format/csv', 'template', 'terminal', 'test', 'time']
MEMBER_HEADERS = ('Method', 'Function', 'Constant', 'Field')
# Sections for the people who make Lakescript, not for the ones who use it
INTERNAL = ('Backlog', "How it's made", 'Later', 'Not doing', 'Still open', 'Being discussed')

# The page of each note, to turn links between notes into links between pages
PAGES = {'README': 'index.html', 'general': 'general.html'}
PAGES.update({t: f'types.{t}.html' for t in TYPES})
PAGES.update({m.rsplit('/', 1)[-1]: m.replace('/', '.') + '.html' for m in MODULES})
TITLES = {'README': 'the standard library', 'general': 'the built-in functions'}
TITLES.update({t: f'`{t}`' for t in TYPES})
TITLES.update({m.rsplit('/', 1)[-1]: f'`std/{m}`' for m in MODULES})


# Where the other notes are read, for links to them *(i.e. ../language/modules.md)*
NOTES_URL = 'https://github.com/Folgue02/lakescript/blob/master/'


# The folder of the note being read, which its links are relative to
reading = [root]


def text(lines):
    """Markdown, with links to the notes of the standard library pointing at
    their pages, and links to the other notes at the notes on GitHub."""
    md = '\n'.join(lines).strip()
    folder = reading[0]

    def link(m):
        label, href = m.group(1), m.group(2)
        if href.startswith(('http://', 'https://')):
            return m.group(0)
        name = Path(href.split('#')[0]).stem
        # A type and a module can share a name *(i.e. `bytes`)*: the folder says which
        target = (folder / href.split('#')[0]).resolve()
        if target.parent.name == 'types' and name in TYPES:
            return f'[{TITLES.get(name, name) if label.endswith(".md") else label}](types.{name}.html)'
        if name not in PAGES:
            target = (folder / href).resolve()
            try:
                where = target.relative_to(repo)
            except ValueError:
                return label
            return f'[{label.removesuffix(".md")}]({NOTES_URL}{where.as_posix()})'
        # `[path.md](path.md)` reads as the page it links to
        if label.endswith('.md'):
            label = TITLES.get(name, name)
        return f'[{label}]({PAGES[name]})'

    return re.sub(r'\[([^\]]+)\]\(([^)]+)\)', link, md)


def cells(line):
    line = line.strip().replace('\\|', '\x01')
    return [c.strip().replace('\x01', '|') for c in line.strip('|').split('|')]


def parse(path):
    """The title, the intro, and the sections: their name, text and rows."""
    reading[0] = path.parent
    lines = path.read_text().splitlines()
    title = lines[0].lstrip('# ').strip()
    intro, sections, cur = [], [], None
    i = 1
    while i < len(lines):
        line = lines[i]
        if line.startswith('## '):
            cur = {'name': line[3:].strip(), 'text': [], 'rows': []}
            sections.append(cur)
        elif line.strip().startswith('|') and cells(line)[0] in MEMBER_HEADERS:
            # A table before the first heading is a section without a name
            unnamed = cur is None
            if unnamed:
                cur = {'name': '', 'text': [], 'rows': []}
                sections.append(cur)
            header = cells(line)[0]
            i += 2  # the header and the `---` line
            while i < len(lines) and lines[i].strip().startswith('|'):
                cur['rows'].append((header, *cells(lines[i])[:3]))
                i += 1
            # What follows it is still the intro
            if unnamed:
                cur = None
            continue
        else:
            (cur['text'] if cur else intro).append(line)
        i += 1
    return title, intro, [s for s in sections if s['name'] not in INTERNAL]


def member(header, name, kind, desc, group=None):
    """A row of a table as an item: its kind, name and declaration."""
    name = name.strip('`')
    kind = kind.strip('`')
    # The name before its parentheses, also with generics *(i.e. `sortBy<K>(...)`)*
    m = re.match(r'([\w?$]+)(?:<[^(]*>)?\(', name)
    short = m.group(1) if m else name
    if header == 'Field':
        return {'kind': 'field', 'name': short, 'code': f'{kind} @{name}', 'text': text([desc]), 'group': group}
    if header == 'Constant':
        return {'kind': 'constant', 'name': short, 'code': f'const {kind} {name}', 'text': text([desc]), 'group': group}
    which = 'method' if header == 'Method' else 'function'
    return {'kind': which, 'name': short, 'code': f'{kind} {name}', 'text': text([desc]), 'group': group}


def module(short, path, title, file):
    """A module: its functions and constants, and its classes."""
    _, intro, sections = parse(file)
    items, groups, notes = [], [], []
    for s in sections:
        if not s['rows']:
            notes.append(f"## {s['name']}\n\n{text(s['text'])}")
            continue
        headers = {r[0] for r in s['rows']}
        if headers & {'Field', 'Method'} and not headers & {'Function'}:
            items.append({'kind': 'class', 'name': s['name'], 'code': f"class {s['name']}", 'text': text(s['text']),
                          'members': [member(*r) for r in s['rows']]})
        else:
            if s['name']:
                groups.append({'name': s['name'], 'text': text(s['text'])})
            items.extend(member(*r, group=s['name'] or None) for r in s['rows'])
    return {'short': short, 'path': path, 'title': title, 'text': text(intro), 'notes': '\n\n'.join(notes),
            'groups': groups, 'items': items}


modules = [module('general', '', 'Built-in functions', root / 'general.md')]
types = []
for name in TYPES:
    _, intro, sections = parse(root / 'types' / f'{name}.md')
    members, groups = [], []
    for s in sections:
        if s['rows']:
            if s['name']:
                groups.append({'name': s['name'], 'text': text(s['text'])})
            members.extend(member(*r, group=s['name'] or None) for r in s['rows'])
    types.append({'kind': 'type', 'name': name, 'code': name, 'text': text(intro), 'members': members, 'groups': groups})
modules.append({'short': 'types', 'path': '', 'title': 'Types', 'text': 'The built-in types and their methods.', 'notes': '',
                'groups': [], 'items': types})
for file in MODULES:
    modules.append(module(file, f'std/{file}', f'std/{file}', root / f'{file}.md'))

# The front page: what's before "How modules are made", which is for the people who make Lakescript
reading[0] = root
readme = (root / 'README.md').read_text().split('\n', 1)[1]
overview = text(readme.split('\n## How modules are made')[0].splitlines())


def toml(value):
    return json.dumps(value, ensure_ascii=False)


lines = ['# Made by tools/docs/stdlib.py from ideas/stdlib. Don\'t change it by hand.', f'overview = {toml(overview)}']


def item(prefix, it):
    lines.append(f'\n[[{prefix}]]')
    for key in ('kind', 'name', 'code', 'text', 'group'):
        if it.get(key):
            lines.append(f'{key} = {toml(it[key])}')
    for g in it.get('groups', []):
        lines.append(f'\n[[{prefix}.groups]]')
        lines.append(f"name = {toml(g['name'])}")
        if g['text']:
            lines.append(f"text = {toml(g['text'])}")
    for m in it.get('members', []):
        item(f'{prefix}.members', m)


for m in modules:
    lines.append('\n[[modules]]')
    for key in ('short', 'path', 'title', 'text', 'notes'):
        if m[key]:
            lines.append(f'{key} = {toml(m[key])}')
    for g in m['groups']:
        lines.append('\n[[modules.groups]]')
        lines.append(f"name = {toml(g['name'])}")
        if g['text']:
            lines.append(f"text = {toml(g['text'])}")
    for it in m['items']:
        item('modules.items', it)

out.parent.mkdir(parents=True, exist_ok=True)
out.write_text('\n'.join(lines) + '\n')
count = sum(len(m['items']) + sum(len(i.get('members', [])) for i in m['items']) for m in modules)
print(f'{out}: {len(modules)} pages of modules, {count} items')
