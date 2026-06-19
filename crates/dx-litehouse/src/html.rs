#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HtmlMetadata {
    pub title_present: bool,
    pub description_present: bool,
    pub canonical_present: bool,
    pub viewport_present: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HtmlSignals {
    pub html_lang_present: bool,
    pub h1_present: bool,
    pub main_landmark_present: bool,
    pub missing_image_alt_count: usize,
    pub missing_button_name_count: usize,
    pub missing_form_label_count: usize,
    pub insecure_reference_count: usize,
}

pub fn inspect_html_metadata(html: &str) -> HtmlMetadata {
    let normalized = html.to_ascii_lowercase();
    HtmlMetadata {
        title_present: has_non_empty_title(&normalized),
        description_present: normalized.contains("name=\"description\"")
            || normalized.contains("name='description'"),
        canonical_present: normalized.contains("rel=\"canonical\"")
            || normalized.contains("rel='canonical'"),
        viewport_present: normalized.contains("name=\"viewport\"")
            || normalized.contains("name='viewport'"),
    }
}

pub fn inspect_html_signals(html: &str) -> HtmlSignals {
    let normalized = html.to_ascii_lowercase();
    HtmlSignals {
        html_lang_present: opening_tag_has_non_empty_attribute(&normalized, "html", "lang"),
        h1_present: tag_has_text(&normalized, "h1"),
        main_landmark_present: normalized.contains("<main") || normalized.contains("role=\"main\""),
        missing_image_alt_count: missing_image_alt_count(&normalized),
        missing_button_name_count: missing_button_name_count(&normalized),
        missing_form_label_count: missing_form_label_count(&normalized),
        insecure_reference_count: insecure_reference_count(&normalized),
    }
}

fn has_non_empty_title(html: &str) -> bool {
    let Some(start_offset) = html.find("<title") else {
        return false;
    };
    let Some(open_end_offset) = html[start_offset..].find('>') else {
        return false;
    };
    let content_start = start_offset + open_end_offset + 1;
    let Some(close_offset) = html[content_start..].find("</title>") else {
        return false;
    };
    !strip_tags(&html[content_start..content_start + close_offset])
        .trim()
        .is_empty()
}

fn missing_image_alt_count(html: &str) -> usize {
    opening_tags(html, "img")
        .filter(|tag| {
            !attribute_has_value(tag, "alt")
                && !attribute_has_value(tag, "aria-label")
                && !attribute_has_value(tag, "aria-labelledby")
        })
        .count()
}

fn missing_button_name_count(html: &str) -> usize {
    let mut count = 0;
    let mut offset = 0;

    while let Some(start_offset) = html[offset..].find("<button") {
        let start = offset + start_offset;
        let Some(open_end_offset) = html[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_offset + 1;
        let tag = &html[start..open_end];
        let has_attribute_name = attribute_has_value(tag, "aria-label")
            || attribute_has_value(tag, "aria-labelledby")
            || attribute_has_value(tag, "title");
        let close = html[open_end..].find("</button>");
        let has_text = close.is_some_and(|close_offset| {
            let inner = &html[open_end..open_end + close_offset];
            !strip_tags(inner).trim().is_empty()
        });

        if !has_attribute_name && !has_text {
            count += 1;
        }
        offset = open_end;
    }

    count
}

fn missing_form_label_count(html: &str) -> usize {
    opening_tags(html, "input")
        .filter(|tag| {
            !is_non_textual_input(tag)
                && !attribute_has_value(tag, "aria-label")
                && !attribute_has_value(tag, "aria-labelledby")
                && !attribute_has_value(tag, "title")
                && !input_has_label(html, tag)
        })
        .count()
}

fn insecure_reference_count(html: &str) -> usize {
    html.match_indices("src=\"http://").count()
        + html.match_indices("src='http://").count()
        + html.match_indices("href=\"http://").count()
        + html.match_indices("href='http://").count()
}

fn input_has_label(html: &str, tag: &str) -> bool {
    let Some(id) = attribute_value(tag, "id") else {
        return false;
    };
    if id.is_empty() {
        return false;
    }
    html.contains(&format!("for=\"{id}\"")) || html.contains(&format!("for='{id}'"))
}

fn is_non_textual_input(tag: &str) -> bool {
    attribute_value(tag, "type").is_some_and(|input_type| {
        matches!(
            input_type,
            "hidden" | "button" | "submit" | "reset" | "image" | "checkbox" | "radio"
        )
    })
}

fn opening_tag_has_non_empty_attribute(html: &str, tag: &str, attribute: &str) -> bool {
    opening_tags(html, tag).any(|tag| attribute_has_value(tag, attribute))
}

fn tag_has_text(html: &str, tag: &str) -> bool {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let Some(start) = html.find(&open) else {
        return false;
    };
    let Some(open_end_offset) = html[start..].find('>') else {
        return false;
    };
    let content_start = start + open_end_offset + 1;
    let Some(close_offset) = html[content_start..].find(&close) else {
        return false;
    };
    !strip_tags(&html[content_start..content_start + close_offset])
        .trim()
        .is_empty()
}

fn opening_tags<'a>(html: &'a str, tag: &str) -> OpeningTags<'a> {
    OpeningTags {
        html,
        needle: format!("<{tag}"),
        offset: 0,
    }
}

struct OpeningTags<'a> {
    html: &'a str,
    needle: String,
    offset: usize,
}

impl<'a> Iterator for OpeningTags<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.html[self.offset..].find(&self.needle)? + self.offset;
        let end = self.html[start..].find('>')? + start + 1;
        self.offset = end;
        Some(&self.html[start..end])
    }
}

fn attribute_has_value(tag: &str, attribute: &str) -> bool {
    attribute_value(tag, attribute).is_some_and(|value| !value.trim().is_empty())
}

fn attribute_value<'a>(tag: &'a str, attribute: &str) -> Option<&'a str> {
    let needle = format!("{attribute}=");
    let start = find_attribute(tag, &needle)? + needle.len();
    let rest = &tag[start..];
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let value_start = quote.len_utf8();
        let value_end = rest[value_start..].find(quote)? + value_start;
        return Some(&rest[value_start..value_end]);
    }

    let value_end = rest
        .find(|character: char| character.is_ascii_whitespace() || character == '>')
        .unwrap_or(rest.len());
    Some(&rest[..value_end])
}

fn find_attribute(tag: &str, needle: &str) -> Option<usize> {
    let mut offset = 0;
    while let Some(found) = tag[offset..].find(needle) {
        let index = offset + found;
        let boundary = tag[..index]
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_whitespace() || character == '<');
        if boundary {
            return Some(index);
        }
        offset = index + needle.len();
    }
    None
}

fn strip_tags(value: &str) -> String {
    let mut text = String::new();
    let mut inside_tag = false;

    for character in value.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => text.push(character),
            _ => {}
        }
    }

    text
}
