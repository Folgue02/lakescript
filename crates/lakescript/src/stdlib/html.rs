//! `std/html/native`: the Rust part of `std/html` *(see `html.lks` and
//! `ideas/stdlib/html.md`)*. Pages are parsed with `scraper`, on html5ever,
//! and a node of a page is a `Value::Handle` that keeps the page alive.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use ego_tree::{NodeId, NodeRef};
use indexmap::IndexMap;
use scraper::{ElementRef, Html, Node, Selector};

use super::{Args, Exports};
use crate::exceptions::{TYPE_ERROR, VALUE_ERROR};
use crate::value::{Key, NativeError, Value};

/// A node of a parsed page: the page, kept alive while one of its nodes is.
struct PageNode {
    page: Rc<Html>,
    id: NodeId,
}

pub fn register(m: &mut Exports) {
    m.function("parse", |_, values| {
        let a = Args::new("html::parse", values, 1, 1)?;
        let page = Rc::new(Html::parse_document(&a.str(0)?));
        let id = page.tree.root().id();
        Ok(handle(page, id))
    });
    m.function("select", |_, values| {
        let a = Args::new("html::select", values, 2, 2)?;
        let (node, selector) = (node(&a, 0)?, selector(&a.str(1)?)?);
        let found = matches(node, &selector).map(|e| handle(node.page.clone(), e.id())).collect();
        Ok(Value::list(found))
    });
    m.function("first", |_, values| {
        let a = Args::new("html::first", values, 2, 2)?;
        let (node, selector) = (node(&a, 0)?, selector(&a.str(1)?)?);
        Ok(matches(node, &selector).next().map(|e| handle(node.page.clone(), e.id())).unwrap_or(Value::Null))
    });
    m.function("tag", |_, values| {
        let a = Args::new("html::tag", values, 1, 1)?;
        let node = node(&a, 0)?;
        Ok(Value::string(element(node).map(|e| e.value().name()).unwrap_or("")))
    });
    m.function("text", |_, values| {
        let a = Args::new("html::text", values, 1, 1)?;
        let node = node(&a, 0)?;
        let mut text = String::new();
        text_of(tree_node(node), &mut text, true);
        Ok(Value::string(&text.split_whitespace().collect::<Vec<_>>().join(" ")))
    });
    m.function("attr", |_, values| {
        let a = Args::new("html::attr", values, 2, 2)?;
        let (node, name) = (node(&a, 0)?, a.str(1)?.to_ascii_lowercase());
        Ok(element(node).and_then(|e| e.value().attr(&name)).map(Value::string).unwrap_or(Value::Null))
    });
    m.function("attrs", |_, values| {
        let a = Args::new("html::attrs", values, 1, 1)?;
        let node = node(&a, 0)?;
        let mut map = IndexMap::new();
        if let Some(e) = element(node) {
            for (name, value) in e.value().attrs() {
                map.insert(Key::Str(Rc::from(name)), Value::string(value));
            }
        }
        Ok(Value::Map(Rc::new(RefCell::new(map))))
    });
    m.function("html", |_, values| {
        let a = Args::new("html::html", values, 1, 1)?;
        let node = node(&a, 0)?;
        Ok(Value::string(&match element(node) {
            Some(e) => e.inner_html(),
            None => node.page.html(),
        }))
    });
    m.function("outerHtml", |_, values| {
        let a = Args::new("html::outerHtml", values, 1, 1)?;
        let node = node(&a, 0)?;
        Ok(Value::string(&match element(node) {
            Some(e) => e.html(),
            None => node.page.html(),
        }))
    });
    m.function("children", |_, values| {
        let a = Args::new("html::children", values, 1, 1)?;
        let node = node(&a, 0)?;
        let children = tree_node(node).children().filter(|c| c.value().is_element()).map(|c| handle(node.page.clone(), c.id())).collect();
        Ok(Value::list(children))
    });
    m.function("parent", |_, values| {
        let a = Args::new("html::parent", values, 1, 1)?;
        let node = node(&a, 0)?;
        Ok(match tree_node(node).parent() {
            Some(p) if p.value().is_element() => handle(node.page.clone(), p.id()),
            _ => Value::Null,
        })
    });
    m.function("escape", |_, values| {
        let a = Args::new("html::escape", values, 1, 1)?;
        let mut out = String::new();
        super::template::escape_html(&a.str(0)?, &mut out);
        Ok(Value::string(&out))
    });
    m.function("unescape", |_, values| {
        let a = Args::new("html::unescape", values, 1, 1)?;
        Ok(Value::string(&unescape(&a.str(0)?)))
    });
}

fn handle(page: Rc<Html>, id: NodeId) -> Value {
    Value::Handle(Rc::new(PageNode { page, id }) as Rc<dyn Any>)
}

/// The node given to a function of `std/html/native`, which only `html.lks` calls.
fn node<'a>(a: &'a Args, i: usize) -> Result<&'a PageNode, NativeError> {
    match a.values.get(i) {
        Some(Value::Handle(h)) => h.downcast_ref::<PageNode>().ok_or_else(|| NativeError::new(TYPE_ERROR, "an `html::Element` or `html::Document` only comes from `html::parse()`")),
        _ => Err(NativeError::new(TYPE_ERROR, "an `html::Element` or `html::Document` only comes from `html::parse()`")),
    }
}

fn tree_node(node: &PageNode) -> NodeRef<'_, Node> {
    node.page.tree.get(node.id).expect("the node is in its page")
}

fn element(node: &PageNode) -> Option<ElementRef<'_>> {
    ElementRef::wrap(tree_node(node))
}

fn selector(css: &str) -> Result<Selector, NativeError> {
    Selector::parse(css).map_err(|e| NativeError::new(VALUE_ERROR, format!("'{css}' isn't a valid CSS selector: {e}")))
}

/// The elements inside `node` that match, in the order of the page. The node
/// itself isn't one of them, as with `querySelectorAll` in browsers.
fn matches<'a>(node: &'a PageNode, selector: &'a Selector) -> impl Iterator<Item = ElementRef<'a>> + 'a {
    tree_node(node).descendants().skip(1).filter_map(ElementRef::wrap).filter(move |e| selector.matches(e))
}

/// Elements whose text isn't shown on the page.
const HIDDEN: &[&str] = &["script", "style", "template", "noscript", "head"];

/// Elements that start a line of their own, so their text doesn't stick to
/// the one around it *(i.e. `<p>a</p><p>b</p>` is `a b`)*.
const BLOCKS: &[&str] = &[
    "address", "article", "aside", "blockquote", "br", "dd", "div", "dl", "dt", "figcaption", "figure", "footer", "form", "h1", "h2", "h3",
    "h4", "h5", "h6", "header", "hr", "li", "main", "nav", "ol", "p", "pre", "section", "table", "td", "th", "tr", "ul",
];

/// The text of a node as a browser shows it, before spaces are collapsed.
/// Hidden elements inside it are left out, but not the node itself, so the
/// text of a `<script>` can still be read *(i.e. the JSON in it)*.
fn text_of(node: NodeRef<'_, Node>, out: &mut String, top: bool) {
    match node.value() {
        Node::Text(text) => out.push_str(text),
        Node::Element(e) if !top && HIDDEN.contains(&e.name()) => {}
        Node::Element(e) => {
            let block = BLOCKS.contains(&e.name());
            if block {
                out.push(' ');
            }
            for child in node.children() {
                text_of(child, out, false);
            }
            if block {
                out.push(' ');
            }
        }
        _ => {
            for child in node.children() {
                text_of(child, out, false);
            }
        }
    }
}

/// Entities turned back into characters *(i.e. `&amp;` into `&`)*: the text
/// is parsed as HTML, with its `<` kept as text.
fn unescape(text: &str) -> String {
    let fragment = Html::parse_fragment(&text.replace('<', "&lt;"));
    fragment.root_element().text().collect()
}
