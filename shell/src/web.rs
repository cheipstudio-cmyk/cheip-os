//! Browser in modalità lettura: scarica la pagina e la riduce a blocchi
//! nativi (titoli, paragrafi, elenchi, link). Pochi MB di RAM invece di un
//! motore web completo, che arriva quando la shell diventa compositor.

use scraper::{ElementRef, Html, Selector};
use std::io::Read;
use url::Url;

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading(String, u8),
    Para(String),
    Item(String),
    Link(String, String),
    Code(String),
    Quote(String),
}

#[derive(Clone, Debug)]
pub struct Page {
    pub url: String,
    pub title: String,
    pub blocks: Vec<Block>,
}

const MAX_BYTES: u64 = 3 * 1024 * 1024;
const MAX_BLOCKS: usize = 500;

/// Testo digitato nella barra: indirizzo o ricerca
pub fn resolve_input(input: &str) -> String {
    let t = input.trim();
    if t.starts_with("http://") || t.starts_with("https://") {
        return t.to_string();
    }
    if !t.contains(' ') && t.contains('.') && !t.ends_with('.') {
        return format!("https://{t}");
    }
    let q: String = percent_encoding::utf8_percent_encode(t, percent_encoding::NON_ALPHANUMERIC).to_string();
    format!("https://html.duckduckgo.com/html/?q={q}")
}

/// I risultati di DuckDuckGo passano da un redirect: estraiamo la destinazione
fn unwrap_redirect(u: &Url) -> Url {
    if u.host_str().map(|h| h.ends_with("duckduckgo.com")).unwrap_or(false) && u.path() == "/l/" {
        if let Some((_, v)) = u.query_pairs().find(|(k, _)| k == "uddg") {
            if let Ok(target) = Url::parse(&v) {
                return target;
            }
        }
    }
    u.clone()
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn inside_skipped(el: &ElementRef) -> bool {
    el.ancestors().filter_map(ElementRef::wrap).any(|a| {
        matches!(a.value().name(), "nav" | "footer" | "header" | "script" | "style" | "noscript" | "aside" | "form")
            || a.value().attr("aria-hidden") == Some("true")
    })
}

pub fn parse(html: &str, base: &str) -> Page {
    let doc = Html::parse_document(html);
    let base_url = Url::parse(base).ok();
    let title = Selector::parse("title")
        .ok()
        .and_then(|s| doc.select(&s).next())
        .map(|t| clean(&t.text().collect::<String>()))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| base.to_string());

    let sel = Selector::parse("h1, h2, h3, h4, p, li, pre, blockquote").expect("selettore");
    let a_sel = Selector::parse("a[href]").expect("selettore");
    let mut blocks = Vec::new();
    let mut seen_links = std::collections::HashSet::new();

    for el in doc.select(&sel) {
        if blocks.len() >= MAX_BLOCKS {
            break;
        }
        if inside_skipped(&el) {
            continue;
        }
        let name = el.value().name();
        // Un <p> dentro <li> o <blockquote> lo prende già il contenitore
        if name == "p" && el.ancestors().filter_map(ElementRef::wrap).any(|a| matches!(a.value().name(), "li" | "blockquote")) {
            continue;
        }
        let text = if name == "pre" { el.text().collect::<String>() } else { clean(&el.text().collect::<String>()) };
        if text.trim().is_empty() {
            continue;
        }
        // Elementi fatti solo di un link: li mostriamo come link
        let links: Vec<(String, String)> = el
            .select(&a_sel)
            .filter_map(|a| {
                let href = a.value().attr("href")?;
                if href.starts_with('#') || href.starts_with("javascript:") || href.starts_with("mailto:") {
                    return None;
                }
                let abs = match &base_url {
                    Some(b) => b.join(href).ok()?,
                    None => Url::parse(href).ok()?,
                };
                let abs = unwrap_redirect(&abs);
                let label = clean(&a.text().collect::<String>());
                (!label.is_empty()).then(|| (label, abs.to_string()))
            })
            .collect();
        let only_link = links.len() == 1 && links[0].0 == text;
        match name {
            _ if only_link => {
                if seen_links.insert(links[0].1.clone()) {
                    blocks.push(Block::Link(links[0].0.clone(), links[0].1.clone()));
                }
                continue;
            }
            "h1" => blocks.push(Block::Heading(text, 1)),
            "h2" => blocks.push(Block::Heading(text, 2)),
            "h3" | "h4" => blocks.push(Block::Heading(text, 3)),
            "li" => blocks.push(Block::Item(text)),
            "pre" => blocks.push(Block::Code(text.trim_end().to_string())),
            "blockquote" => blocks.push(Block::Quote(text)),
            _ => blocks.push(Block::Para(text)),
        }
        for (label, href) in links.into_iter().take(4) {
            if seen_links.insert(href.clone()) {
                blocks.push(Block::Link(label, href));
            }
        }
    }
    Page { url: base.to_string(), title, blocks }
}

pub fn fetch(url: &str) -> Result<Page, String> {
    let resp = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (Linux; Cheip OS) CheipReader/0.2")
        .set("Accept-Language", "it-IT,it;q=0.9,en;q=0.6")
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => format!("Il sito ha risposto {code}"),
            _ => "Sito non raggiungibile".to_string(),
        })?;
    let final_url = resp.get_url().to_string();
    let ctype = resp.content_type().to_string();
    let mut buf = Vec::new();
    resp.into_reader().take(MAX_BYTES).read_to_end(&mut buf).map_err(|_| "Download interrotto".to_string())?;
    let body = String::from_utf8_lossy(&buf);
    if ctype.starts_with("text/plain") {
        return Ok(Page { url: final_url.clone(), title: final_url, blocks: vec![Block::Code(body.to_string())] });
    }
    if !ctype.contains("html") {
        return Err(format!("Tipo di file non supportato ({ctype})"));
    }
    Ok(parse(&body, &final_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"<html><head><title> Cheip   News </title></head><body>
<nav><a href="/menu">Menu</a></nav>
<h1>Titolo</h1>
<p>Primo paragrafo con <a href="/art/2">un link</a> dentro.</p>
<ul><li><a href="https://esempio.it/x">Solo link</a></li><li>Voce <b>uno</b></li></ul>
<blockquote><p>Citazione</p></blockquote>
<pre>let x = 1;
let y = 2;</pre>
<footer><p>Copyright</p></footer>
<p><a href="javascript:void(0)">no</a></p>
</body></html>"#;

    #[test]
    fn parse_blocks() {
        let p = parse(HTML, "https://cheipstudio.org/blog/");
        assert_eq!(p.title, "Cheip News");
        assert_eq!(p.blocks[0], Block::Heading("Titolo".into(), 1));
        assert_eq!(p.blocks[1], Block::Para("Primo paragrafo con un link dentro.".into()));
        assert_eq!(p.blocks[2], Block::Link("un link".into(), "https://cheipstudio.org/art/2".into()));
        assert_eq!(p.blocks[3], Block::Link("Solo link".into(), "https://esempio.it/x".into()));
        assert_eq!(p.blocks[4], Block::Item("Voce uno".into()));
        assert_eq!(p.blocks[5], Block::Quote("Citazione".into()));
        assert!(matches!(&p.blocks[6], Block::Code(c) if c.contains('\n')));
        assert!(!p.blocks.iter().any(|b| matches!(b, Block::Para(t) if t == "Copyright")), "footer escluso");
        assert!(!p.blocks.iter().any(|b| matches!(b, Block::Link(t, _) if t == "Menu")), "nav esclusa");
    }

    #[test]
    fn input_resolution() {
        assert_eq!(resolve_input("cheipstudio.org"), "https://cheipstudio.org");
        assert_eq!(resolve_input("http://a.b/c"), "http://a.b/c");
        assert_eq!(resolve_input("meteo sesto calende"), "https://html.duckduckgo.com/html/?q=meteo%20sesto%20calende");
    }

    #[test]
    fn ddg_redirect() {
        let u = Url::parse("https://duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.comune.it%2Fpagina&rut=abc").unwrap();
        assert_eq!(unwrap_redirect(&u).as_str(), "https://www.comune.it/pagina");
    }
}
