use std::collections::BTreeMap;

use html_escape::decode_html_entities;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkItem {
    pub id: u32,
    pub title: String,
    pub assigned_to: String,
    pub state: String,
    pub work_item_type: String,
    pub description: String,
    pub acceptance_criteria: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailField {
    Title,
    Dynamic(usize),
}

lazy_static! {
    /// Regex to strip HTML tags; use replacement logic to preserve <img>
    static ref HTML_TAG_REGEX: Regex = Regex::new(r"<[^>]*>").unwrap();
}

pub fn clean_ado_text(input: &str) -> String {
    let decoded_text = decode_html_entities(input).to_string();
    let stripped_text = HTML_TAG_REGEX
        .replace_all(&decoded_text, |caps: &regex::Captures| {
            let tag = &caps[0];
            let trimmed = tag
                .trim_start_matches('<')
                .trim_start_matches('/')
                .split(|c| c == ' ' || c == '>' || c == '/')
                .next()
                .unwrap_or("");

            if trimmed.eq_ignore_ascii_case("img") {
                tag.to_string()
            } else {
                String::new()
            }
        })
        .to_string();

    stripped_text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_ado_text_strips_simple_html() {
        let input = "<div>Hello <b>world</b></div>";
        assert_eq!(clean_ado_text(input), "Hello world");
    }

    #[test]
    fn test_clean_ado_text_preserves_img_tags() {
        let input = "<img src='test.png'> Description";
        assert_eq!(clean_ado_text(input), "<img src='test.png'> Description");
    }

    #[test]
    fn test_clean_ado_text_preserves_img_with_attributes() {
        let input = "Text <img src=\"image.png\" alt=\"test\" /> more text";
        assert_eq!(
            clean_ado_text(input),
            "Text <img src=\"image.png\" alt=\"test\" /> more text"
        );
    }

    #[test]
    fn test_clean_ado_text_empty_string() {
        assert_eq!(clean_ado_text(""), "");
    }

    #[test]
    fn test_clean_ado_text_no_html() {
        let input = "Plain text without HTML";
        assert_eq!(clean_ado_text(input), "Plain text without HTML");
    }

    #[test]
    fn test_clean_ado_text_nested_tags() {
        let input = "<div><p>Text</p></div>";
        assert_eq!(clean_ado_text(input), "Text");
    }

    #[test]
    fn test_clean_ado_text_trims_whitespace() {
        let input = "  <div>Text</div>  ";
        assert_eq!(clean_ado_text(input), "Text");
    }

    #[test]
    fn test_clean_ado_text_decodes_html_entities() {
        let input = "&lt;div&gt;Text&lt;/div&gt;";
        assert_eq!(clean_ado_text(input), "Text");
    }

    #[test]
    fn test_clean_ado_text_multiple_img_tags() {
        let input = "<img src='a.png'> Text <img src='b.png'>";
        assert_eq!(
            clean_ado_text(input),
            "<img src='a.png'> Text <img src='b.png'>"
        );
    }

    #[test]
    fn test_clean_ado_text_self_closing_tags() {
        let input = "Line 1<br/>Line 2<hr/>Line 3";
        // Self-closing tags are stripped without replacement, so no spaces between
        assert_eq!(clean_ado_text(input), "Line 1Line 2Line 3");
    }

    #[test]
    fn test_detail_field_equality() {
        assert_eq!(DetailField::Title, DetailField::Title);
        assert_ne!(DetailField::Title, DetailField::Dynamic(0));
        assert_eq!(DetailField::Dynamic(5), DetailField::Dynamic(5));
        assert_ne!(DetailField::Dynamic(5), DetailField::Dynamic(6));
    }

    #[test]
    fn test_work_item_default_fields() {
        let item = WorkItem {
            id: 123,
            title: "Test".to_string(),
            assigned_to: "Alice".to_string(),
            state: "Active".to_string(),
            work_item_type: "Bug".to_string(),
            description: "Desc".to_string(),
            acceptance_criteria: "AC".to_string(),
            fields: BTreeMap::new(),
        };

        assert_eq!(item.id, 123);
        assert_eq!(item.title, "Test");
        assert!(item.fields.is_empty());
    }
}
