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

const SHADOW_FIXTURE: &str = r#"@layer properties;
@layer utilities {
  .shadow-cta {
    --tw-shadow: 0 4px 12px var(--tw-shadow-color, rgb(7 193 96 / 0.3));
    box-shadow: var(--tw-inset-shadow), var(--tw-inset-ring-shadow), var(--tw-ring-offset-shadow), var(--tw-ring-shadow), var(--tw-shadow);
  }
}
@layer properties {
  @property --tw-shadow {
    syntax: "*";
    inherits: false;
    initial-value: 0 0 #0000;
  }
  @property --tw-shadow-color {
    syntax: "*";
    inherits: false;
  }
  @property --tw-inset-shadow {
    syntax: "*";
    inherits: false;
    initial-value: 0 0 #0000;
  }
  @property --tw-inset-ring-shadow {
    syntax: "*";
    inherits: false;
    initial-value: 0 0 #0000;
  }
  @property --tw-ring-offset-shadow {
    syntax: "*";
    inherits: false;
    initial-value: 0 0 #0000;
  }
  @property --tw-ring-shadow {
    syntax: "*";
    inherits: false;
    initial-value: 0 0 #0000;
  }
}
"#;

#[test]
fn local_tw_var_referencing_earlier_local_resolves_in_rule_order() {
    let css = "@layer utilities {\n  .x {\n    --tw-shadow-color: #07c160;\n    --tw-shadow: 0 4px 12px var(--tw-shadow-color, transparent);\n    box-shadow: var(--tw-shadow);\n  }\n}\n";
    let out = postprocess_v4(css);
    assert!(
        out.css.contains("box-shadow: 0 4px 12px #07c160"),
        "css:\n{}",
        out.css
    );
}

#[test]
fn locally_declared_tw_shadow_wins_over_property_initial() {
    let out = postprocess_v4(SHADOW_FIXTURE);
    assert!(
        out.css.contains("box-shadow: 0 0 #0000, 0 0 #0000, 0 0 #0000, 0 0 #0000, 0 4px 12px rgb(7 193 96 / 0.3)"),
        "css:\n{}",
        out.css
    );
    assert!(!out.css.contains("box-shadow: 0 0 #0000, 0 0 #0000, 0 0 #0000, 0 0 #0000, 0 0 #0000"), "css:\n{}", out.css);
}

#[test]
fn user_theme_tokens_generate_utilities_when_available() {
    let classes: Vec<String> =
        ["bg-primary", "text-cell", "pb-safe"].iter().map(|s| s.to_string()).collect();
    let theme = "@theme {\n  --color-primary: #07c160;\n  --text-cell: 17px;\n}\n@utility pb-safe {\n  padding-bottom: env(safe-area-inset-bottom);\n}\n";
    let result = match mistc::tailwind_cli::generate(&classes, Some(theme)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping: no tailwind CLI available ({})", e);
            return;
        }
    };
    assert!(result.css.contains(".bg-primary"), "css:\n{}", result.css);
    assert!(result.theme_css.contains("--color-primary: #07c160"), "theme:\n{}", result.theme_css);
    assert!(result.css.contains(".text-cell"), "css:\n{}", result.css);
    assert!(result.css.contains(".pb-safe"), "css:\n{}", result.css);
    assert!(result.css.contains("env(safe-area-inset-bottom)"), "css:\n{}", result.css);
}

#[test]
fn real_v4_cli_end_to_end_when_available() {
    let classes: Vec<String> =
        ["p-4", "bg-blue-500", "md:flex"].iter().map(|s| s.to_string()).collect();
    let result = match mistc::tailwind_cli::generate(&classes, None) {
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

// verbatim @tailwindcss/cli v4.3.3 output for:
// bg-gradient-to-br from-[#f97316] to-[#fbbf24]
const GRADIENT_2STOP_FIXTURE: &str = r#"/*! tailwindcss v4.3.3 | MIT License | https://tailwindcss.com */
@layer properties;
@layer utilities {
  .bg-gradient-to-br {
    --tw-gradient-position: to bottom right in oklab;
    background-image: linear-gradient(var(--tw-gradient-stops));
  }
  .from-\[\#f97316\] {
    --tw-gradient-from: #f97316;
    --tw-gradient-stops: var(--tw-gradient-via-stops, var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-to) var(--tw-gradient-to-position));
  }
  .to-\[\#fbbf24\] {
    --tw-gradient-to: #fbbf24;
    --tw-gradient-stops: var(--tw-gradient-via-stops, var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-to) var(--tw-gradient-to-position));
  }
}
@property --tw-gradient-position {
  syntax: "*";
  inherits: false;
}
@property --tw-gradient-from {
  syntax: "<color>";
  inherits: false;
  initial-value: #0000;
}
@property --tw-gradient-via {
  syntax: "<color>";
  inherits: false;
  initial-value: #0000;
}
@property --tw-gradient-to {
  syntax: "<color>";
  inherits: false;
  initial-value: #0000;
}
@property --tw-gradient-stops {
  syntax: "*";
  inherits: false;
}
@property --tw-gradient-via-stops {
  syntax: "*";
  inherits: false;
}
@property --tw-gradient-from-position {
  syntax: "<length-percentage>";
  inherits: false;
  initial-value: 0%;
}
@property --tw-gradient-via-position {
  syntax: "<length-percentage>";
  inherits: false;
  initial-value: 50%;
}
@property --tw-gradient-to-position {
  syntax: "<length-percentage>";
  inherits: false;
  initial-value: 100%;
}
@layer properties {
  @supports ((-webkit-hyphens: none) and (not (margin-trim: inline))) or ((-moz-orient: inline) and (not (color:rgb(from red r g b)))) {
    *, ::before, ::after, ::backdrop {
      --tw-gradient-position: initial;
      --tw-gradient-from: #0000;
      --tw-gradient-via: #0000;
      --tw-gradient-to: #0000;
      --tw-gradient-stops: initial;
      --tw-gradient-via-stops: initial;
      --tw-gradient-from-position: 0%;
      --tw-gradient-via-position: 50%;
      --tw-gradient-to-position: 100%;
    }
  }
}
"#;

// verbatim @tailwindcss/cli v4.3.3 output for:
// bg-gradient-to-br from-[#f97316] via-[#eab308] to-[#fbbf24]
const GRADIENT_3STOP_FIXTURE: &str = r#"/*! tailwindcss v4.3.3 | MIT License | https://tailwindcss.com */
@layer properties;
@layer utilities {
  .bg-gradient-to-br {
    --tw-gradient-position: to bottom right in oklab;
    background-image: linear-gradient(var(--tw-gradient-stops));
  }
  .from-\[\#f97316\] {
    --tw-gradient-from: #f97316;
    --tw-gradient-stops: var(--tw-gradient-via-stops, var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-to) var(--tw-gradient-to-position));
  }
  .via-\[\#eab308\] {
    --tw-gradient-via: #eab308;
    --tw-gradient-via-stops: var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-via) var(--tw-gradient-via-position), var(--tw-gradient-to) var(--tw-gradient-to-position);
    --tw-gradient-stops: var(--tw-gradient-via-stops);
  }
  .to-\[\#fbbf24\] {
    --tw-gradient-to: #fbbf24;
    --tw-gradient-stops: var(--tw-gradient-via-stops, var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-to) var(--tw-gradient-to-position));
  }
}
@property --tw-gradient-position {
  syntax: "*";
  inherits: false;
}
@property --tw-gradient-from {
  syntax: "<color>";
  inherits: false;
  initial-value: #0000;
}
@property --tw-gradient-via {
  syntax: "<color>";
  inherits: false;
  initial-value: #0000;
}
@property --tw-gradient-to {
  syntax: "<color>";
  inherits: false;
  initial-value: #0000;
}
@property --tw-gradient-stops {
  syntax: "*";
  inherits: false;
}
@property --tw-gradient-via-stops {
  syntax: "*";
  inherits: false;
}
@property --tw-gradient-from-position {
  syntax: "<length-percentage>";
  inherits: false;
  initial-value: 0%;
}
@property --tw-gradient-via-position {
  syntax: "<length-percentage>";
  inherits: false;
  initial-value: 50%;
}
@property --tw-gradient-to-position {
  syntax: "<length-percentage>";
  inherits: false;
  initial-value: 100%;
}
@layer properties {
  @supports ((-webkit-hyphens: none) and (not (margin-trim: inline))) or ((-moz-orient: inline) and (not (color:rgb(from red r g b)))) {
    *, ::before, ::after, ::backdrop {
      --tw-gradient-position: initial;
      --tw-gradient-from: #0000;
      --tw-gradient-via: #0000;
      --tw-gradient-to: #0000;
      --tw-gradient-stops: initial;
      --tw-gradient-via-stops: initial;
      --tw-gradient-from-position: 0%;
      --tw-gradient-via-position: 50%;
      --tw-gradient-to-position: 100%;
    }
  }
}
"#;

#[test]
fn gradient_two_stop_resolves_via_runtime_cascade_not_unset() {
    let out = postprocess_v4(GRADIENT_2STOP_FIXTURE);
    assert!(
        out.css.contains("background-image: linear-gradient(var(--tw-gradient-stops))"),
        "css:\n{}",
        out.css
    );
    assert!(out.css.contains("--tw-gradient-from: #f97316"), "css:\n{}", out.css);
    assert!(out.css.contains("--tw-gradient-to: #fbbf24"), "css:\n{}", out.css);
    assert!(!out.css.contains("unset"), "css:\n{}", out.css);
    assert!(!out.css.contains("linear-gradient(unset)"), "css:\n{}", out.css);
}

#[test]
fn gradient_position_color_interpolation_hint_is_stripped() {
    let out = postprocess_v4(GRADIENT_2STOP_FIXTURE);
    assert!(!out.css.contains(" in oklab"), "css:\n{}", out.css);
    assert!(!out.css.contains("in oklab"), "css:\n{}", out.css);
    assert!(
        out.css.contains("--tw-gradient-position: to bottom right;"),
        "css:\n{}",
        out.css
    );
}

#[test]
fn gradient_initial_values_land_on_page_for_inheritance() {
    let out = postprocess_v4(GRADIENT_2STOP_FIXTURE);
    assert!(out.theme_css.contains("page {"), "theme:\n{}", out.theme_css);
    assert!(out.theme_css.contains("--tw-gradient-from: #0000"), "theme:\n{}", out.theme_css);
    assert!(out.theme_css.contains("--tw-gradient-to: #0000"), "theme:\n{}", out.theme_css);
    assert!(out.theme_css.contains("--tw-gradient-from-position: 0%"), "theme:\n{}", out.theme_css);
    assert!(out.theme_css.contains("--tw-gradient-via-position: 50%"), "theme:\n{}", out.theme_css);
    assert!(out.theme_css.contains("--tw-gradient-to-position: 100%"), "theme:\n{}", out.theme_css);
    assert!(!out.css.contains("page {"), "css:\n{}", out.css);
}

#[test]
fn gradient_three_stop_via_resolves_without_unset() {
    let out = postprocess_v4(GRADIENT_3STOP_FIXTURE);
    assert!(out.css.contains("--tw-gradient-via: #eab308"), "css:\n{}", out.css);
    assert!(
        out.css.contains(
            "--tw-gradient-via-stops: var(--tw-gradient-position), var(--tw-gradient-from) var(--tw-gradient-from-position), var(--tw-gradient-via) var(--tw-gradient-via-position), var(--tw-gradient-to) var(--tw-gradient-to-position)"
        ),
        "css:\n{}",
        out.css
    );
    assert!(!out.css.contains("unset"), "css:\n{}", out.css);
    assert!(!out.css.contains("in oklab"), "css:\n{}", out.css);
}

#[test]
fn non_gradient_tw_vars_still_fully_resolved_no_leftover() {
    let out = postprocess_v4(GRADIENT_2STOP_FIXTURE);
    for line in out.css.lines() {
        if line.contains("--tw-") {
            assert!(line.contains("gradient"), "unexpected leftover --tw- var: {}", line);
        }
    }
}

#[test]
fn real_v4_cli_gradient_end_to_end_when_available() {
    let classes: Vec<String> = ["bg-gradient-to-br", "from-[#f97316]", "to-[#fbbf24]"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let result = match mistc::tailwind_cli::generate(&classes, None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping: no tailwind CLI available ({})", e);
            return;
        }
    };
    assert!(
        result.css.contains("background-image: linear-gradient(var(--tw-gradient-stops))"),
        "css:\n{}",
        result.css
    );
    assert!(!result.css.contains("unset"), "css:\n{}", result.css);
    assert!(!result.css.contains("in oklab"), "css:\n{}", result.css);
    assert!(result.theme_css.contains("--tw-gradient-from: #0000"), "theme:\n{}", result.theme_css);
}
