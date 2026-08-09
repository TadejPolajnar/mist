use mistc::tailwind_cli::postprocess;

#[test]
fn sanitizes_escaped_selectors_consistently_with_markup() {
    let css = ".w-\\[32px\\] {\n  width: 32px\n}\n.bg-black\\/50 {\n  background-color: rgb(0 0 0 / 0.5)\n}\n";
    let out = postprocess(css);
    assert!(out.css.contains(".w-_32px_ {"), "css:\n{}", out.css);
    assert!(out.css.contains(".bg-black_50 {"), "css:\n{}", out.css);
    assert!(out.dropped_selectors.is_empty(), "dropped: {:?}", out.dropped_selectors);
}

#[test]
fn converts_rem_to_rpx() {
    let css = ".p-4 {\n  padding: 1rem\n}\n.mt-1 {\n  margin-top: 0.25rem\n}\n";
    let out = postprocess(css);
    assert!(out.css.contains("padding: 32rpx"), "css:\n{}", out.css);
    assert!(out.css.contains("margin-top: 8rpx"), "css:\n{}", out.css);
}

#[test]
fn keeps_media_queries_with_sanitized_inner_selectors() {
    let css = "@media (min-width: 768px) {\n  .md\\:flex {\n    display: flex\n  }\n}\n";
    let out = postprocess(css);
    assert!(out.css.contains("@media (min-width: 768px) {"), "css:\n{}", out.css);
    assert!(out.css.contains(".md_flex {"), "css:\n{}", out.css);
}

#[test]
fn drops_selectors_wxss_cannot_express() {
    let css = ".hover\\:bg-red-500:hover {\n  background: red\n}\n.space-x-2 > :not([hidden]) ~ :not([hidden]) {\n  margin-left: 0.5rem\n}\n.flex {\n  display: flex\n}\n";
    let out = postprocess(css);
    assert!(out.css.contains(".flex {"), "css:\n{}", out.css);
    assert!(!out.css.contains(":hover"), "css:\n{}", out.css);
    assert!(!out.css.contains(":not("), "css:\n{}", out.css);
    assert_eq!(out.dropped_selectors.len(), 2, "dropped: {:?}", out.dropped_selectors);
}

#[test]
fn pseudo_class_variants_without_combinators_are_dropped() {
    let css = ".checked\\:bg-blue-500:checked {\n  background: blue\n}\n.focus-visible\\:ring:focus-visible {\n  outline: 1px\n}\n.first\\:mt-0:first-child {\n  margin-top: 0\n}\n.flex {\n  display: flex\n}\n";
    let out = postprocess(css);
    assert!(out.css.contains(".flex {"), "css:\n{}", out.css);
    assert!(!out.css.contains(":checked"), "css:\n{}", out.css);
    assert!(!out.css.contains(":focus-visible"), "css:\n{}", out.css);
    assert_eq!(out.dropped_selectors.len(), 3, "dropped: {:?}", out.dropped_selectors);
}

#[test]
fn grouped_and_pseudo_element_selectors_are_kept() {
    let css = ".a, .b {\n  color: red\n}\npage {\n  --x: 1\n}\n.icon::before {\n  content: 'x'\n}\n";
    let out = postprocess(css);
    assert!(out.css.contains(".a, .b {"), "css:\n{}", out.css);
    assert!(out.theme_css.contains("page {"), "theme:\n{}", out.theme_css);
    assert!(out.css.contains(".icon::before {"), "css:\n{}", out.css);
    assert!(out.dropped_selectors.is_empty(), "dropped: {:?}", out.dropped_selectors);
}

#[test]
fn container_queries_are_dropped_not_passed_through() {
    let css = "@container (min-width: 100px) {\n  .cq\\:flex {\n    display: flex\n  }\n}\n.flex {\n  display: flex\n}\n";
    let out = postprocess(css);
    assert!(!out.css.contains("@container"), "css:\n{}", out.css);
    assert!(out.css.contains(".flex {"), "css:\n{}", out.css);
    assert!(out.dropped_selectors.iter().any(|d| d.contains("@container")), "dropped: {:?}", out.dropped_selectors);
}

#[test]
fn border_utilities_get_style_and_color_without_preflight() {
    let css = ".border {\n  border-width: 1px\n}\n";
    let out = postprocess(css);
    assert!(out.css.contains("border-style: solid"), "css:\n{}", out.css);
    assert!(out.css.contains("border-color: #e5e7eb"), "css:\n{}", out.css);
}

#[test]
fn strips_comments() {
    let css = "/*! tailwindcss v3.4.17 */\n.flex {\n  display: flex\n}\n";
    let out = postprocess(css);
    assert!(!out.css.contains("tailwindcss v3"), "css:\n{}", out.css);
    assert!(out.css.contains(".flex {"), "css:\n{}", out.css);
}

#[test]
fn real_cli_end_to_end_when_available() {
    let classes: Vec<String> = ["p-4", "md:flex", "w-[32px]", "space-x-2", "text-lg", "bg-black/50"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let result = match mistc::tailwind_cli::generate(&classes, None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping: tailwind CLI unavailable ({})", e);
            return;
        }
    };
    assert!(result.css.contains(".md_flex"), "css:\n{}", result.css);
    assert!(result.css.contains("width: 32px"), "css:\n{}", result.css);
    assert!(result.css.contains(".bg-black_50"), "css:\n{}", result.css);
    // space-x generates sibling selectors — must be dropped
    assert!(!result.css.contains(":not("), "css:\n{}", result.css);
    assert!(!result.dropped_selectors.is_empty(), "expected space-x rule dropped");
    // rem never survives
    assert!(!result.css.contains("rem"), "css:\n{}", result.css);
    assert!(result.css.contains("padding: calc(var(--spacing) * 4)"), "css:\n{}", result.css);
    assert!(result.theme_css.contains("--text-lg: 36rpx"), "theme:\n{}", result.theme_css);
}
