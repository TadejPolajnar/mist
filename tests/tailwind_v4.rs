use mistc::tailwind_cli::postprocess_v4;

// verbatim @tailwindcss/cli v4.3.3 output for:
// p-4 flex text-2xl bg-blue-500 bg-black/50 md:flex w-[32px] rounded-lg border text-gray-400
const V4_FIXTURE: &str = r#"/*! tailwindcss v4.3.3 | MIT License | https://tailwindcss.com */
@layer properties;
@layer theme {
  :root, :host {
    --color-blue-500: oklch(62.3% 0.214 259.815);
    --color-gray-400: oklch(70.7% 0.022 261.325);
    --color-black: #000;
    --spacing: 0.25rem;
    --text-2xl: 1.5rem;
    --text-2xl--line-height: calc(2 / 1.5);
    --radius-lg: 0.5rem;
  }
}
@layer utilities {
  .flex {
    display: flex;
  }
  .w-\[32px\] {
    width: 32px;
  }
  .rounded-lg {
    border-radius: var(--radius-lg);
  }
  .border {
    border-style: var(--tw-border-style);
    border-width: 1px;
  }
  .bg-black\/50 {
    background-color: color-mix(in srgb, #000 50%, transparent);
    @supports (color: color-mix(in lab, red, red)) {
      background-color: color-mix(in oklab, var(--color-black) 50%, transparent);
    }
  }
  .bg-blue-500 {
    background-color: var(--color-blue-500);
  }
  .p-4 {
    padding: calc(var(--spacing) * 4);
  }
  .text-2xl {
    font-size: var(--text-2xl);
    line-height: var(--tw-leading, var(--text-2xl--line-height));
  }
  .text-gray-400 {
    color: var(--color-gray-400);
  }
  @media (width >= 48rem) {
    .md\:flex {
      display: flex;
    }
  }
}
@property --tw-border-style {
  syntax: "*";
  inherits: false;
  initial-value: solid;
}
@layer properties {
  @supports ((-webkit-hyphens: none) and (not (margin-trim: inline))) or ((-moz-orient: inline) and (not (color:rgb(from red r g b)))) {
    *, ::before, ::after, ::backdrop {
      --tw-border-style: solid;
    }
  }
}
"#;

#[test]
fn theme_block_becomes_page_with_rpx_and_hex() {
    let out = postprocess_v4(V4_FIXTURE);
    assert!(out.theme_css.contains("page {"), "theme:\n{}", out.theme_css);
    assert!(!out.css.contains("page {"), "css:\n{}", out.css);
    assert!(!out.css.contains(":root"), "css:\n{}", out.css);
    assert!(out.theme_css.contains("--spacing: 8rpx"), "theme:\n{}", out.theme_css);
    assert!(out.theme_css.contains("--text-2xl: 48rpx"), "theme:\n{}", out.theme_css);
    // oklch(62.3% 0.214 259.815) is tailwind v4 blue-500 (#2b7fff)
    assert!(out.theme_css.contains("--color-blue-500: #2b7fff"), "theme:\n{}", out.theme_css);
}

#[test]
fn no_layers_or_property_blocks_survive() {
    let out = postprocess_v4(V4_FIXTURE);
    assert!(!out.css.contains("@layer"), "css:\n{}", out.css);
    assert!(!out.css.contains("@property"), "css:\n{}", out.css);
    assert!(!out.css.contains("*,"), "css:\n{}", out.css);
    assert!(!out.css.contains("::backdrop"), "css:\n{}", out.css);
}

#[test]
fn tw_vars_resolved_from_property_initials_and_fallbacks() {
    let out = postprocess_v4(V4_FIXTURE);
    assert!(out.css.contains("border-style: solid"), "css:\n{}", out.css);
    assert!(out.css.contains("line-height: var(--text-2xl--line-height)"), "css:\n{}", out.css);
    assert!(!out.css.contains("--tw-"), "css:\n{}", out.css);
}

#[test]
fn color_mix_with_transparent_precomputed() {
    let out = postprocess_v4(V4_FIXTURE);
    assert!(out.css.contains(".bg-black_50 {"), "css:\n{}", out.css);
    assert!(out.css.contains("background-color: rgba(0, 0, 0, 0.5)"), "css:\n{}", out.css);
    assert!(!out.css.contains("color-mix"), "css:\n{}", out.css);
}

#[test]
fn media_range_syntax_converted() {
    let out = postprocess_v4(V4_FIXTURE);
    assert!(out.css.contains("@media (min-width: 768px)"), "css:\n{}", out.css);
    assert!(out.css.contains(".md_flex"), "css:\n{}", out.css);
}

#[test]
fn arbitrary_value_selector_sanitized() {
    let out = postprocess_v4(V4_FIXTURE);
    assert!(out.css.contains(".w-_32px_ {"), "css:\n{}", out.css);
}

#[test]
fn real_v4_cli_end_to_end_when_available() {
    let classes: Vec<String> =
        ["p-4", "bg-blue-500", "md:flex"].iter().map(|s| s.to_string()).collect();
    let result = match mistc::tailwind_cli::generate(&classes) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping: no tailwind CLI available ({})", e);
            return;
        }
    };
    assert!(result.theme_css.contains("page {"), "theme:\n{}", result.theme_css);
    assert!(result.css.contains(".p-4"), "css:\n{}", result.css);
    assert!(result.theme_css.contains("#2b7fff"), "theme:\n{}", result.theme_css);
    assert!(!result.css.contains("@layer"), "css:\n{}", result.css);
    assert!(!result.css.contains("oklch"), "css:\n{}", result.css);
}
