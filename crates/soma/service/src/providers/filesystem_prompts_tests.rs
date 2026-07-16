use tempfile::tempdir;

use super::*;

#[test]
fn prompt_name_from_file_stem_slugifies_and_lowercases() {
    assert_eq!(prompt_name_from_file_stem("Code Review"), "code-review");
    assert_eq!(prompt_name_from_file_stem("code_review"), "code-review");
    assert_eq!(prompt_name_from_file_stem("code-review"), "code-review");
}

#[test]
fn prompt_name_from_file_stem_falls_back_when_empty_or_non_lowercase_start() {
    assert_eq!(prompt_name_from_file_stem("---"), "prompt");
    assert_eq!(prompt_name_from_file_stem(""), "prompt");
    assert_eq!(prompt_name_from_file_stem("123abc"), "prompt-123abc");
}

#[test]
fn first_markdown_heading_finds_first_h1_only() {
    assert_eq!(
        first_markdown_heading("intro\n# Title\nmore text\n# Second"),
        Some("Title".to_owned())
    );
    assert_eq!(first_markdown_heading("## Not H1\nbody"), None);
    assert_eq!(first_markdown_heading("no heading here"), None);
    assert_eq!(
        first_markdown_heading("#    "),
        None,
        "blank heading text is not a heading"
    );
}

#[test]
fn load_markdown_catalog_value_derives_name_description_and_template() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("Code Review.md");
    std::fs::write(&path, "# Code Review\n\nCheck it.\n").expect("write file");

    let value = load_markdown_catalog_value(&path).expect("load markdown catalog");
    assert_eq!(value["provider"]["name"], "code-review");
    assert_eq!(value["provider"]["kind"], "static-rust");
    assert_eq!(value["prompts"][0]["name"], "code-review");
    assert_eq!(value["prompts"][0]["description"], "Code Review");
    assert_eq!(
        value["prompts"][0]["template"],
        "# Code Review\n\nCheck it.\n"
    );
}

#[test]
fn load_markdown_catalog_value_falls_back_to_generated_description_without_heading() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("notes.md");
    std::fs::write(&path, "just some text, no heading\n").expect("write file");

    let value = load_markdown_catalog_value(&path).expect("load markdown catalog");
    assert_eq!(
        value["prompts"][0]["description"],
        "Markdown prompt from notes"
    );
}
