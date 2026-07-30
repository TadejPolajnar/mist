use mistc::tailwind::sanitize;

fn page(template: &str) -> String {
    format!("---\nimport {{ state }} from 'mist'\nconst n = state(0)\n---\n{}\n", template)
}

#[test]
fn sanitize_special_characters() {
    assert_eq!(sanitize("md:flex"), "md_flex");
    assert_eq!(sanitize("w-[32px]"), "w-_32px_");
    assert_eq!(sanitize("bg-black/50"), "bg-black_50");
    assert_eq!(sanitize("p-4"), "p-4");
}

#[test]
fn wxml_class_names_sanitized_and_wxss_imports_shared() {
    let unit = mistc::compile_unit(&page("<div class=\"p-4 w-[32px]\"><span class={n.value > 0 ? 'bg-black/50' : ''}>x</span></div>"), true)
        .expect("compile failed");
    assert!(unit.output.wxml.contains("class=\"p-4 w-_32px_\""), "wxml:\n{}", unit.output.wxml);
    assert!(unit.output.wxml.contains("'bg-black_50'"), "wxml:\n{}", unit.output.wxml);
    assert!(unit.output.wxss.contains("@import \"./tw-shared.wxss\";"), "wxss:\n{}", unit.output.wxss);
    assert_eq!(unit.classes, vec!["bg-black/50".to_string(), "p-4".to_string(), "w-[32px]".to_string()]);
}

#[test]
fn no_classes_means_no_import() {
    let unit = mistc::compile_unit(&page("<span>{n.value}</span>"), true).expect("compile failed");
    assert!(!unit.output.wxss.contains("@import"), "wxss:\n{}", unit.output.wxss);
}

#[test]
fn project_collects_shared_css_across_units() {
    let project = mistc::compile_project(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/app/index.mist"
    )))
    .expect("project compile failed");
    // classes from the page…
    assert!(project.tailwind_css.contains(".p-4"), "css:\n{}", project.tailwind_css);
    assert!(project.tailwind_css.contains(".text-2xl"), "css:\n{}", project.tailwind_css);
    // …and from components (stateful and inlined)
    assert!(project.tailwind_css.contains(".py-2"), "css:\n{}", project.tailwind_css);
    assert!(project.tailwind_css.contains(".line-through"), "css:\n{}", project.tailwind_css);
    assert!(project.unknown_classes.is_empty(), "unknown: {:?}", project.unknown_classes);
    assert!(project.dropped_selectors.is_empty(), "dropped: {:?}", project.dropped_selectors);
}
