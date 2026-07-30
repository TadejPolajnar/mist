#[test]
fn m1004_reports_file_line_of_the_mutation() {
    // line 1: ---, line 2: import, line 3: const, line 4: function, line 5: splice
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\nfunction rm(i) {\n  items.value.splice(i, 1)\n}\n---\n<span>{items.value.length}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1004 at line 5:3"), "err: {}", err);
    assert!(err.contains("help:"), "err: {}", err);
}

#[test]
fn m1003_computed_key_errors() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map(t => (<span key={t.a + t.b}>{t.a}</span>))}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1003"), "err: {}", err);
    assert!(err.contains("direct property"), "err: {}", err);
}

#[test]
fn m1003_deep_key_path_errors() {
    let src = "---\nimport { state } from 'mist'\nconst items = state([])\n---\n<div>{items.value.map(t => (<span key={t.meta.id}>{t.a}</span>))}</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1003"), "err: {}", err);
}

#[test]
fn template_errors_report_file_line() {
    // template starts on line 5; the bad closing tag is on line 7
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\n---\n<div>\n  <span>{n.value}\n</div>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1010 at line 7"), "err: {}", err);
}

#[test]
fn m1005_collision_is_coded() {
    let src = "---\nimport { state } from 'mist'\nconst n = state(0)\nfunction n() {}\n---\n<span>{n.value}</span>\n";
    let err = mistc::compile(src).unwrap_err();
    assert!(err.contains("M1005"), "err: {}", err);
}

#[test]
fn line_col_helper_is_accurate() {
    let src = "abc\ndefg\nhi";
    assert_eq!(mistc::frontmatter::line_col(src, 0, 1), (1, 1));
    assert_eq!(mistc::frontmatter::line_col(src, 4, 1), (2, 1));
    assert_eq!(mistc::frontmatter::line_col(src, 6, 1), (2, 3));
    assert_eq!(mistc::frontmatter::line_col(src, 9, 10), (12, 1));
}
