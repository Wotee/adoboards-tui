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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailField {
    Title,
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
