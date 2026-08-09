//! RSS and Atom, turned into items.
//!
//! Pure parsing: give it the XML, get back the entries. Fetching is the
//! caller's business — kastrup shells out to curl, the phone uses its own
//! HTTP client, and neither belongs in here.
//!
//! Deliberately not a full XML parser. Feeds in the wild are ill-formed
//! often enough that a strict one is a liability, and the shape being
//! read is shallow: find a tag, take what is between it and its close.
//! CDATA and the five XML entities are handled; namespaces are ignored,
//! which is why `dc:creator` and `content:encoded` work by name.

/// One entry from a feed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Item {
    /// Stable across fetches: the entry's own id, else its link, else its
    /// title. Feeds reorder and repaginate, so position means nothing.
    pub id: String,
    pub title: String,
    /// The entry's author if it names one, else the feed's title.
    pub author: String,
    pub link: String,
    /// Unix seconds, 0 when the feed gives no usable date.
    pub published: i64,
    /// The entry's content as it arrived, HTML and all.
    pub html: String,
    /// The same with the tags taken out, for a list row or a search.
    pub text: String,
    pub feed_title: String,
    pub feed_url: String,
}

/// Parse a feed. Atom and RSS both, told apart by the document element.
pub fn parse(xml: &str, feed_title: &str, feed_url: &str) -> Vec<Item> {
    let is_atom = xml.contains("<feed") && xml.contains("http://www.w3.org/2005/Atom");
    let (splitter, out) = if is_atom { ("<entry", true) } else { ("<item", false) };

    xml.split(splitter).skip(1).map(|part| {
        let (title, link, content, author, date, id) = if out {
            (
                extract_tag(part, "title"),
                extract_attr(part, "link", "href").or_else(|| extract_tag(part, "link")),
                extract_tag(part, "content").or_else(|| extract_tag(part, "summary")),
                extract_tag(part, "name").or_else(|| extract_tag(part, "author")),
                extract_tag(part, "updated").or_else(|| extract_tag(part, "published")),
                extract_tag(part, "id"),
            )
        } else {
            (
                extract_tag(part, "title"),
                extract_tag(part, "link"),
                extract_tag(part, "description").or_else(|| extract_tag(part, "content:encoded")),
                extract_tag(part, "author").or_else(|| extract_tag(part, "dc:creator")),
                extract_tag(part, "pubDate"),
                extract_tag(part, "guid"),
            )
        };
        let content = content.unwrap_or_default();
        // An author can arrive as markup: MyBB's dc:creator is a whole
        // anchor tag round the name. Left alone it reaches a list row as
        // `<a href=…` and gets shortened to nonsense.
        let author = author.map(|a| strip_html_simple(&a).trim().to_string())
            .filter(|a| !a.is_empty());
        Item {
            id: format!("rss_{}", simple_hash(
                id.as_deref().or(link.as_deref()).unwrap_or(title.as_deref().unwrap_or("")))),
            title: title.unwrap_or_default(),
            author: author.unwrap_or_else(|| feed_title.to_string()),
            link: link.unwrap_or_default(),
            published: date.as_deref().and_then(parse_date).unwrap_or(0),
            text: strip_html_simple(&content),
            html: content,
            feed_title: feed_title.to_string(),
            feed_url: feed_url.to_string(),
        }
    }).collect()
}

/// RFC 2822 (`Thu, 3 Apr 2026 09:15:00 +0200`) or ISO 8601
/// (`2026-04-03T09:15:00Z`), whichever the feed felt like.
pub fn parse_date(date_str: &str) -> Option<i64> {
    let s = date_str.trim();
    if s.is_empty() { return None; }
    parse_rfc2822(s).or_else(|| parse_iso8601(s))
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)?;
    let after_open = xml[start..].find('>')?;
    let content_start = start + after_open + 1;
    let end = xml[content_start..].find(&close)?;
    let content = &xml[content_start..content_start + end];
    // Decode CDATA
    let content = if content.starts_with("<![CDATA[") && content.ends_with("]]>") {
        &content[9..content.len()-3]
    } else {
        content
    };
    Some(decode_entities(content.trim()))
}

fn extract_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let pattern = format!("<{}", tag);
    let start = xml.find(&pattern)?;
    let tag_end = xml[start..].find('>')?;
    let tag_content = &xml[start..start + tag_end];
    let attr_pattern = format!("{}=\"", attr);
    let attr_start = tag_content.find(&attr_pattern)?;
    let value_start = attr_start + attr_pattern.len();
    let value_end = tag_content[value_start..].find('"')?;
    Some(tag_content[value_start..value_start + value_end].to_string())
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
        .replace("&quot;", "\"").replace("&apos;", "'").replace("&#39;", "'")
}

pub fn strip_html_simple(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        if ch == '<' { in_tag = true; continue; }
        if ch == '>' { in_tag = false; continue; }
        if !in_tag { result.push(ch); }
    }
    decode_entities(&result)
}

fn parse_rfc2822(s: &str) -> Option<i64> {
    // Strip day name if present
    let s = if let Some(pos) = s.find(',') { s[pos+1..].trim() } else { s };

    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 4 { return None; }

    let day: i64 = parts[0].parse().ok()?;
    let month = match parts[1].to_lowercase().as_str() {
        "jan" => 1, "feb" => 2, "mar" => 3, "apr" => 4,
        "may" => 5, "jun" => 6, "jul" => 7, "aug" => 8,
        "sep" => 9, "oct" => 10, "nov" => 11, "dec" => 12,
        _ => return None,
    };
    let year: i64 = parts[2].parse().ok()?;

    let time_parts: Vec<&str> = parts[3].split(':').collect();
    let hour: i64 = time_parts.get(0)?.parse().ok()?;
    let min: i64 = time_parts.get(1)?.parse().ok()?;
    let sec: i64 = time_parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    let tz_offset: i64 = if let Some(tz) = parts.get(4) {
        let tz = tz.trim();
        if tz.len() >= 4 && (tz.starts_with('+') || tz.starts_with('-')) {
            let sign: i64 = if tz.starts_with('-') { -1 } else { 1 };
            let h: i64 = tz[1..3].parse().unwrap_or(0);
            let m: i64 = tz[3..5].parse().unwrap_or(0);
            sign * (h * 3600 + m * 60)
        } else {
            0
        }
    } else {
        0
    };

    let mut y = year;
    let mut m = month as i64;
    if m <= 2 { y -= 1; m += 12; }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    Some(days * 86400 + hour * 3600 + min * 60 + sec - tz_offset)
}

fn parse_iso8601(s: &str) -> Option<i64> {
    // "2026-04-03T09:15:00Z" or "2026-04-03T09:15:00+02:00"
    let (datetime, tz_part) = if s.ends_with('Z') {
        (&s[..s.len()-1], "+00:00")
    } else if s.len() > 6 {
        let last6 = &s[s.len()-6..];
        if (last6.starts_with('+') || last6.starts_with('-')) && last6.contains(':') {
            (&s[..s.len()-6], last6)
        } else {
            (s, "+00:00")
        }
    } else {
        (s, "+00:00")
    };

    let date_time: Vec<&str> = datetime.split('T').collect();
    if date_time.len() < 2 { return None; }

    let date_parts: Vec<&str> = date_time[0].split('-').collect();
    if date_parts.len() < 3 { return None; }
    let year: i64 = date_parts[0].parse().ok()?;
    let month: i64 = date_parts[1].parse().ok()?;
    let day: i64 = date_parts[2].parse().ok()?;

    let time_parts: Vec<&str> = date_time[1].split(':').collect();
    let hour: i64 = time_parts.get(0)?.parse().ok()?;
    let min: i64 = time_parts.get(1)?.parse().ok()?;
    let sec: i64 = time_parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    // Parse timezone offset
    let tz_offset: i64 = {
        let tz = tz_part.trim();
        if tz.len() >= 5 && (tz.starts_with('+') || tz.starts_with('-')) {
            let sign: i64 = if tz.starts_with('-') { -1 } else { 1 };
            let parts: Vec<&str> = tz[1..].split(':').collect();
            let h: i64 = parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0);
            let m: i64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            sign * (h * 3600 + m * 60)
        } else {
            0
        }
    };

    let mut y = year;
    let mut m = month;
    if m <= 2 { y -= 1; m += 12; }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m - 3) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    Some(days * 86400 + hour * 3600 + min * 60 + sec - tz_offset)
}

fn simple_hash(s: &str) -> String {
    let mut h: u64 = 5381;
    for b in s.bytes() { h = h.wrapping_mul(33).wrapping_add(b as u64); }
    format!("{:016x}", h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_rss_item_comes_apart() {
        let xml = r#"<rss><channel><item>
            <title>Hello &amp; welcome</title>
            <link>https://example.com/1</link>
            <description><![CDATA[<p>Some <b>body</b></p>]]></description>
            <pubDate>Thu, 3 Apr 2026 09:15:00 +0200</pubDate>
            <guid>tag:1</guid></item></channel></rss>"#;
        let items = parse(xml, "Example", "https://example.com/feed");
        assert_eq!(items.len(), 1);
        let i = &items[0];
        assert_eq!(i.title, "Hello & welcome");
        assert_eq!(i.link, "https://example.com/1");
        assert_eq!(i.text.trim(), "Some body");
        assert!(i.html.contains("<b>"));
        assert_eq!(i.author, "Example", "no author element: the feed stands in");
        assert_eq!(i.published, 1775200500);
    }

    #[test]
    fn an_atom_entry_comes_apart() {
        let xml = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry>
            <title>Post</title>
            <link href="https://example.com/2"/>
            <updated>2026-04-03T09:15:00Z</updated>
            <author><name>Someone</name></author>
            <content>Words</content>
            <id>urn:2</id></entry></feed>"#;
        let items = parse(xml, "Example", "u");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Post");
        assert_eq!(items[0].link, "https://example.com/2", "atom puts it in an attribute");
        assert_eq!(items[0].author, "Someone");
        assert_eq!(items[0].published, 1775207700);
    }

    #[test]
    fn the_id_is_stable_and_prefers_the_guid() {
        let with = parse("<rss><item><guid>g</guid><link>l</link></item></rss>", "f", "u");
        let without = parse("<rss><item><link>l</link></item></rss>", "f", "u");
        assert_ne!(with[0].id, without[0].id);
        let again = parse("<rss><item><guid>g</guid><link>OTHER</link></item></rss>", "f", "u");
        assert_eq!(with[0].id, again[0].id, "a changed link must not orphan the entry");
    }

    #[test]
    fn an_author_wrapped_in_markup_comes_back_as_a_name() {
        // MyBB (the HP Forum) puts a whole anchor in dc:creator.
        let xml = "<rss><item><title>T</title><dc:creator><![CDATA[\
            <a href=\"https://x/member.php?uid=1\">Eddie W. Shore</a>]]></dc:creator>\
            </item></rss>";
        assert_eq!(parse(xml, "HP Forum", "u")[0].author, "Eddie W. Shore");
    }

    #[test]
    fn an_author_of_only_markup_falls_back_to_the_feed() {
        let xml = "<rss><item><title>T</title><dc:creator><![CDATA[<br/>]]></dc:creator></item></rss>";
        assert_eq!(parse(xml, "HP Forum", "u")[0].author, "HP Forum");
    }

    #[test]
    fn a_feed_with_no_dates_still_parses() {
        let items = parse("<rss><item><title>T</title></item></rss>", "f", "u");
        assert_eq!(items[0].published, 0);
    }

    #[test]
    fn both_date_spellings_agree() {
        assert_eq!(parse_date("Thu, 3 Apr 2026 07:15:00 +0000"),
                   parse_date("2026-04-03T07:15:00Z"));
    }

    #[test]
    fn nothing_at_all_yields_nothing() {
        assert!(parse("", "f", "u").is_empty());
        assert!(parse("<html><body>not a feed</body></html>", "f", "u").is_empty());
    }
}
