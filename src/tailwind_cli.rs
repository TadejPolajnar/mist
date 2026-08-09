//! Real Tailwind CLI integration: run `tailwindcss` over the extracted class list,
//! then post-process the CSS for WXSS — selector sanitization (matching the markup
//! rewriting in `tailwind::sanitize`), rem → rpx (1rem = 32rpx), and removal of
//! selectors WXSS cannot express (reported, never silently dropped).

use crate::tailwind::sanitize_char;
use regex::Regex;
use std::fs;
use std::process::Command;

pub struct CliCss {
    pub css: String,
    /// `page { ... }` theme-variable rules — legal only in page/app WXSS, so they
    /// ship in a separate sheet that components never import
    pub theme_css: String,
    /// rules removed because WXSS cannot express their selector
    pub dropped_selectors: Vec<String>,
}

/// Generate CSS for the given classes with the Tailwind v4 CLI
/// (`@tailwindcss/cli`, installed once into a persistent cache). Results are
/// memoized per class set, so watch-mode rebuilds that change no classes skip
/// the subprocess entirely.
pub fn generate(classes: &[String], theme: Option<&str>) -> Result<CliCss, String> {
    let cache = css_memo_dir().join(format!("css-{:016x}", class_hash(classes, theme)));
    if let Some(hit) = read_cached(&cache) {
        return Ok(hit);
    }
    let raw = run_cli_v4(classes, theme)?;
    let result = postprocess_v4(&raw);
    write_cached(&cache, &result);
    Ok(result)
}

/// Bump when `postprocess_v4` output changes — invalidates memoized CSS.
const CSS_CACHE_REV: u32 = 4;

fn class_hash(classes: &[String], theme: Option<&str>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut h);
    CSS_CACHE_REV.hash(&mut h);
    for c in classes {
        c.hash(&mut h);
    }
    theme.hash(&mut h);
    h.finish()
}

fn read_cached(dir: &std::path::Path) -> Option<CliCss> {
    let css = fs::read_to_string(dir.join("css.wxss")).ok()?;
    let theme_css = fs::read_to_string(dir.join("theme.wxss")).ok()?;
    let dropped = fs::read_to_string(dir.join("dropped.txt")).ok()?;
    Some(CliCss {
        css,
        theme_css,
        dropped_selectors: dropped.lines().map(String::from).filter(|l| !l.is_empty()).collect(),
    })
}

fn write_cached(dir: &std::path::Path, result: &CliCss) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let _ = fs::write(dir.join("css.wxss"), &result.css);
    let _ = fs::write(dir.join("theme.wxss"), &result.theme_css);
    let _ = fs::write(dir.join("dropped.txt"), result.dropped_selectors.join("\n"));
}

/// Persistent npm dir so `npm install` runs once, not per build. Lives under
/// ~/.cache (macOS purges $TMPDIR periodically); temp dir is the fallback.
fn v4_cache_dir() -> std::path::PathBuf {
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => {
            std::path::PathBuf::from(home).join(".cache").join("mistc").join("tw4")
        }
        _ => std::env::temp_dir().join("mistc-tw4"),
    }
}

/// Memoized CSS lives OUTSIDE the CLI's working dir — Tailwind v4's automatic
/// content detection scans the cwd, and cached selectors would leak back in as
/// phantom class usage.
fn css_memo_dir() -> std::path::PathBuf {
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => {
            std::path::PathBuf::from(home).join(".cache").join("mistc").join("css")
        }
        _ => std::env::temp_dir().join("mistc-css"),
    }
}

fn bundled_cli() -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(std::env::var_os("MISTC_TAILWIND_CLI")?);
    path.exists().then_some(path)
}

fn bundled_root(cli: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut p = cli;
    while let Some(parent) = p.parent() {
        if parent.file_name().is_some_and(|n| n == "node_modules") {
            return parent.parent().map(std::path::Path::to_path_buf);
        }
        p = parent;
    }
    None
}

fn run_cli_v4(classes: &[String], theme: Option<&str>) -> Result<String, String> {
    let dir = v4_cache_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with("css-") {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    let bundled = bundled_cli();
    if bundled.is_none() && !dir.join("node_modules/@tailwindcss/cli").exists() {
        eprintln!("installing Tailwind v4 CLI (first run, ~20 MB, needs network)…");
        fs::write(dir.join("package.json"), "{ \"name\": \"mistc-tw\", \"private\": true }")
            .map_err(|e| e.to_string())?;
        // pinned to v4 — the postprocessor is written against v4 output shape
        let out = Command::new("npm")
            .args(["install", "--no-audit", "--no-fund", "tailwindcss@^4", "@tailwindcss/cli@^4"])
            .current_dir(&dir)
            .output()
            .map_err(|e| format!("npm not available: {}", e))?;
        if !out.status.success() {
            return Err(format!("npm install failed: {}", String::from_utf8_lossy(&out.stderr)));
        }
    }
    // Tailwind resolves `@import "tailwindcss/…"` relative to the importing file,
    // not the cwd — input.css must sit where node_modules is reachable upward
    let io_root = bundled.as_deref().and_then(bundled_root).unwrap_or_else(|| dir.clone());
    // per-invocation IO dir — concurrent builds must not interleave files
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let io = io_root.join(format!(
        "io-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir_all(&io).map_err(|e| e.to_string())?;
    fs::write(io.join("content.html"), format!("<div class=\"{}\"></div>", classes.join(" ")))
        .map_err(|e| e.to_string())?;
    let mut input_css = String::from(
        "@import \"tailwindcss/theme.css\" layer(theme);\n@import \"tailwindcss/utilities.css\" layer(utilities) source(none);\n@source \"./content.html\";\n",
    );
    if let Some(theme_src) = theme {
        input_css.push_str(theme_src);
        input_css.push('\n');
    }
    fs::write(io.join("input.css"), input_css).map_err(|e| e.to_string())?;
    let input = io.join("input.css");
    let output = io.join("out.css");
    let mut cmd = match &bundled {
        Some(cli) => {
            let mut c = Command::new("node");
            c.arg(cli);
            c
        }
        None => {
            let mut c = Command::new("npx");
            c.arg("@tailwindcss/cli");
            c
        }
    };
    let out = cmd
        .arg("-i")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .current_dir(&io_root)
        .output()
        .map_err(|e| format!("@tailwindcss/cli failed to start: {}", e))?;
    if !out.status.success() {
        return Err(format!("@tailwindcss/cli failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let css = fs::read_to_string(&output).map_err(|e| e.to_string());
    let _ = fs::remove_dir_all(&io);
    css
}

/// v4 output → WXSS: unwrap `@layer`, `:root,:host` → `page`, resolve
/// `@property` defaults into `var(--tw-*)` uses, precompute `oklch()` and
/// `color-mix(... , transparent)`, modern media ranges → min/max-width px,
/// then the shared selector/rem pipeline.
///
/// `--tw-gradient-*` is the one family left unresolved: `from-*`/`via-*`/`to-*`
/// and `bg-gradient-to-*` are separate utility classes stacked on the same
/// element, so the chain only resolves through the real CSS cascade (same
/// mechanism as theme vars — see `page {}` inheritance below), not
/// same-rule string substitution. Their `@property` initial-values are
/// emitted as plain `page { --tw-gradient-from: #0000; ... }` declarations
/// (inherited by every component, exactly like `--color-*` theme vars) and
/// their `var(--tw-gradient-*)` references are left intact for the runtime
/// cascade to combine.
pub fn postprocess_v4(css: &str) -> CliCss {
    let comments = Regex::new(r"(?s)/\*.*?\*/").unwrap();
    let css = comments.replace_all(css, "").to_string();

    let (css, initials) = extract_property_initials(&css);
    let (initials, gradient_initials) = split_gradient_initials(initials);
    let gradient_initials = strip_gradient_interpolation_hints_map(gradient_initials);
    let css = strip_gradient_interpolation_hints(&css);
    let css = css.replace("@layer properties;", "");
    let css = unwrap_layers(&css);
    let css = css.replace(":root, :host", "page");
    let css = substitute_tw_vars_scoped(&css, &initials);
    let css = convert_oklch(&css);
    let css = convert_color_mix(&css);
    let css = convert_media_ranges(&css);
    // v4 `rounded-full`; WXSS can't parse infinity
    let css = css.replace("calc(infinity * 1px)", "9999px");

    let mut out = String::new();
    let mut dropped = Vec::new();
    process_block(&css, &mut out, &mut dropped, "");
    let (css, mut theme_css) = split_theme(&out);
    if !gradient_initials.is_empty() {
        theme_css.push_str(&gradient_page_rule(&gradient_initials));
    }
    CliCss { css, theme_css, dropped_selectors: dropped }
}

/// Pull `--tw-gradient-*` out of the resolved `@property` initials map — they
/// must NOT be substituted into `var(--tw-gradient-*)` call sites (that would
/// re-introduce the same-rule-only resolution bug); they get a `page {}`
/// declaration instead so the runtime cascade resolves them across rules.
fn split_gradient_initials(
    initials: std::collections::BTreeMap<String, String>,
) -> (std::collections::BTreeMap<String, String>, std::collections::BTreeMap<String, String>) {
    let mut rest = std::collections::BTreeMap::new();
    let mut gradient = std::collections::BTreeMap::new();
    for (k, v) in initials {
        if k.starts_with("--tw-gradient-") {
            gradient.insert(k, v);
        } else {
            rest.insert(k, v);
        }
    }
    (rest, gradient)
}

fn gradient_page_rule(initials: &std::collections::BTreeMap<String, String>) -> String {
    let mut out = String::from("page {\n");
    for (name, value) in initials {
        out.push_str(&format!("  {}: {};\n", name, value));
    }
    out.push_str("}\n");
    out
}

/// v4 emits `--tw-gradient-position: to bottom right in oklab;` — the
/// `in <color-space>` interpolation hint is CSS Color 4 (~Chrome 113+) and
/// unparseable on older WeChat webviews, which invalidates the whole
/// `linear-gradient()` and silently drops the background. Same treatment as
/// `oklch()` → hex: strip what older engines can't parse, don't ship it raw.
/// sRGB (the pre-Color-4 default) is what's left after the hint is gone.
fn gradient_interpolation_hint_re() -> Regex {
    Regex::new(r"\s+in\s+(oklab|oklch|srgb-linear|srgb|hsl|hwb|lab|lch|xyz-d50|xyz-d65|xyz)(\s+(shorter|longer|increasing|decreasing)\s+hue)?").unwrap()
}

fn strip_gradient_interpolation_hints(css: &str) -> String {
    let hint = gradient_interpolation_hint_re();
    let decl = Regex::new(r"(?m)(--tw-gradient-[\w-]+\s*:[^;]*);").unwrap();
    decl.replace_all(css, |caps: &regex::Captures| {
        format!("{};", hint.replace_all(&caps[1], ""))
    })
    .to_string()
}

fn strip_gradient_interpolation_hints_map(
    initials: std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    let hint = gradient_interpolation_hint_re();
    initials.into_iter().map(|(k, v)| (k, hint.replace_all(&v, "").to_string())).collect()
}

/// Pull top-level `page { ... }` rules out into their own sheet — tag selectors
/// are not allowed in component WXSS, and every component imports the shared sheet.
fn split_theme(css: &str) -> (String, String) {
    let mut kept = String::new();
    let mut theme = String::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let prelude = &rest[..open];
        let after = &rest[open + 1..];
        let close = matching_close(after);
        let item_end = (open + 1 + close + 1).min(rest.len());
        let item = rest[..item_end].trim_start_matches('\n');
        let target = if prelude.trim() == "page" { &mut theme } else { &mut kept };
        target.push_str(item);
        target.push('\n');
        rest = rest[item_end..].strip_prefix('\n').unwrap_or(&rest[item_end..]);
    }
    let tail = rest.trim();
    if !tail.is_empty() {
        kept.push_str(tail);
        kept.push('\n');
    }
    (kept, theme)
}

/// Collect `@property --x { initial-value: v }` and remove the blocks.
fn extract_property_initials(css: &str) -> (String, std::collections::BTreeMap<String, String>) {
    let mut map = std::collections::BTreeMap::new();
    let re = Regex::new(r"(?s)@property\s+(--[\w-]+)\s*\{[^}]*?initial-value:\s*([^;}]+);?[^}]*\}").unwrap();
    for caps in re.captures_iter(css) {
        map.insert(caps[1].to_string(), caps[2].trim().to_string());
    }
    let stripped = Regex::new(r"(?s)@property\s+--[\w-]+\s*\{[^}]*\}").unwrap();
    (stripped.replace_all(css, "").to_string(), map)
}

/// `@layer theme { X }` → `X` (one level).
fn unwrap_layers(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(pos) = rest.find("@layer") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos..];
        let Some(open) = after.find('{') else {
            rest = &after["@layer".len()..];
            out.push_str("@layer");
            continue;
        };
        let body_start = pos + open + 1;
        let close = matching_close(&rest[body_start..]);
        out.push_str(&rest[body_start..body_start + close]);
        rest = &rest[body_start + close + 1..];
    }
    out.push_str(rest);
    out
}

fn substitute_tw_vars_scoped(css: &str, initials: &std::collections::BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let close = open + matching_brace(&rest[open..]);
        let body = &rest[open + 1..close];
        out.push_str(&rest[..open + 1]);
        if body.contains('{') {
            out.push_str(&substitute_tw_vars_scoped(body, initials));
        } else {
            let mut scoped = initials.clone();
            for decl in body.split(';') {
                let decl = decl.trim();
                if decl.starts_with("--tw-") {
                    if let Some(colon) = decl.find(':') {
                        let value = substitute_tw_vars(decl[colon + 1..].trim(), &scoped);
                        scoped.insert(decl[..colon].trim().to_string(), value);
                    }
                }
            }
            out.push_str(&substitute_tw_vars(body, &scoped));
        }
        out.push('}');
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

fn matching_brace(s: &str) -> usize {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len()
}

/// Resolve `var(--tw-*)` references: `@property` initial value wins, else the
/// declared fallback, else the whole declaration is dropped later.
///
/// `--tw-gradient-*` is skipped (left as a literal `var(...)` call) — its
/// chain only resolves through the runtime cascade across sibling utility
/// classes, see the `postprocess_v4` doc comment.
fn substitute_tw_vars(css: &str, initials: &std::collections::BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(pos) = rest.find("var(--tw-") {
        out.push_str(&rest[..pos]);
        let open = pos + 3; // at '('
        let close = matching_paren(&rest[open..]);
        let inner = &rest[open + 1..open + close];
        let (name, fallback) = match top_level_comma(inner) {
            Some(i) => (inner[..i].trim(), Some(inner[i + 1..].trim())),
            None => (inner.trim(), None),
        };
        if name.starts_with("--tw-gradient-") {
            out.push_str(&rest[pos..open + close + 1]);
            rest = &rest[open + close + 1..];
            continue;
        }
        match initials.get(name) {
            Some(v) => out.push_str(v),
            None => match fallback {
                Some(f) => out.push_str(&substitute_tw_vars(f, initials)),
                None => out.push_str("unset"),
            },
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

fn matching_paren(s: &str) -> usize {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len()
}

fn top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// `oklch(62.3% 0.214 259.815)` → `#3b82f6`-style hex (WXSS-safe).
fn convert_oklch(css: &str) -> String {
    let re = Regex::new(r"oklch\(([\d.]+)%\s+([\d.]+)\s+([\d.]+)(?:\s*/\s*([\d.]+%?))?\)").unwrap();
    re.replace_all(css, |caps: &regex::Captures| {
        let l: f64 = caps[1].parse().unwrap_or(0.0);
        let c: f64 = caps[2].parse().unwrap_or(0.0);
        let h: f64 = caps[3].parse().unwrap_or(0.0);
        let (r, g, b) = oklch_to_srgb(l / 100.0, c, h);
        match caps.get(4) {
            Some(a) => {
                let alpha = a.as_str().trim_end_matches('%').parse::<f64>().unwrap_or(100.0);
                let alpha = if a.as_str().ends_with('%') { alpha / 100.0 } else { alpha };
                format!("rgba({}, {}, {}, {})", r, g, b, alpha)
            }
            None => format!("#{:02x}{:02x}{:02x}", r, g, b),
        }
    })
    .to_string()
}

fn oklch_to_srgb(l: f64, c: f64, h: f64) -> (u8, u8, u8) {
    let hr = h.to_radians();
    let a = c * hr.cos();
    let b = c * hr.sin();
    let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m_ = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s_ = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    let lin_r = 4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_;
    let lin_g = -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_;
    let lin_b = -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_;
    let gamma = |x: f64| -> u8 {
        let x = x.clamp(0.0, 1.0);
        let v = if x > 0.0031308 { 1.055 * x.powf(1.0 / 2.4) - 0.055 } else { 12.92 * x };
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    (gamma(lin_r), gamma(lin_g), gamma(lin_b))
}

/// `color-mix(in srgb, #000 50%, transparent)` → `rgba(0, 0, 0, 0.5)`.
/// Non-literal mixes (var refs, two colors) are left as-is.
fn convert_color_mix(css: &str) -> String {
    let re = Regex::new(
        r"color-mix\(in\s+\w+,\s*#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})\s+([\d.]+)%\s*,\s*transparent\)",
    )
    .unwrap();
    re.replace_all(css, |caps: &regex::Captures| {
        let hex = &caps[1];
        let pct: f64 = caps[2].parse().unwrap_or(100.0);
        let expand: Vec<u8> = if hex.len() == 3 {
            hex.chars().map(|c| u8::from_str_radix(&format!("{}{}", c, c), 16).unwrap_or(0)).collect()
        } else {
            (0..3).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0)).collect()
        };
        format!("rgba({}, {}, {}, {})", expand[0], expand[1], expand[2], pct / 100.0)
    })
    .to_string()
}

/// `(width >= 48rem)` → `(min-width: 768px)`; `(width < 48rem)` → `(max-width: 767px)`.
fn convert_media_ranges(css: &str) -> String {
    let re = Regex::new(r"\(width\s*(>=|<=|>|<)\s*([\d.]+)rem\)").unwrap();
    re.replace_all(css, |caps: &regex::Captures| {
        let px = caps[2].parse::<f64>().unwrap_or(0.0) * 16.0;
        match &caps[1] {
            ">=" => format!("(min-width: {}px)", px as i64),
            ">" => format!("(min-width: {}px)", px as i64 + 1),
            "<=" => format!("(max-width: {}px)", px as i64),
            _ => format!("(max-width: {}px)", px as i64 - 1),
        }
    })
    .to_string()
}

/// Final shared pass: selector sanitization/dropping and rem → rpx over plain
/// (already-flattened) CSS. Exposed for tests.
pub fn postprocess(css: &str) -> CliCss {
    let comments = Regex::new(r"(?s)/\*.*?\*/").unwrap();
    let css = comments.replace_all(css, "");
    let mut out = String::new();
    let mut dropped = Vec::new();
    process_block(&css, &mut out, &mut dropped, "");
    let (css, theme_css) = split_theme(&out);
    CliCss { css, theme_css, dropped_selectors: dropped }
}

/// Handles one level of `selector { body }` items, recursing into at-rule blocks.
fn process_block(css: &str, out: &mut String, dropped: &mut Vec<String>, indent: &str) {
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let prelude = rest[..open].trim().to_string();
        let after = &rest[open + 1..];
        let close = matching_close(after);
        let body = &after[..close];
        rest = &after[close + 1..];

        if prelude.starts_with('@') {
            if prelude.starts_with("@media") || prelude.starts_with("@supports") {
                let mut inner = String::new();
                process_block(body, &mut inner, dropped, "  ");
                if !inner.trim().is_empty() {
                    out.push_str(&format!("{} {{\n{}}}\n", prelude, inner));
                }
            } else if prelude.starts_with("@keyframes") {
                out.push_str(&format!("{} {{{}}}\n", prelude, body));
            } else {
                // @container etc — WXSS cannot express these
                dropped.push(prelude);
            }
            continue;
        }

        match transform_selector(&prelude) {
            Some(selector) => {
                // v4 nests @supports variants inside rule bodies — WXSS can't;
                // keep only the plain declarations before the nested block
                let body = match body.find('@') {
                    Some(at) => body[..at].trim_end().trim_end_matches(';'),
                    None => body.trim_end(),
                };
                let mut body = convert_rem(body);
                // preflight (unavailable in WXSS — needs `*`) normally supplies
                // border-style/color for border-width utilities
                if body.contains("border-width") && !body.contains("border-style") {
                    body.push_str(";\n  border-style: solid;\n  border-color: #e5e7eb");
                }
                out.push_str(&format!("{}{} {{{} }}\n", indent, selector, body));
            }
            None => {
                // internal var-bootstrap rules (only `--*` declarations) are
                // expected casualties, not worth a user-facing warning
                let only_custom_props = body
                    .split(';')
                    .map(str::trim)
                    .filter(|d| !d.is_empty())
                    .all(|d| d.starts_with("--"));
                if !only_custom_props {
                    dropped.push(unescape(&prelude));
                }
            }
        }
    }
}

fn matching_close(s: &str) -> usize {
    let mut depth = 1;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len()
}

/// Sanitize escaped characters (part of the class name) and reject selectors WXSS
/// cannot express. Returns None when the rule must be dropped.
///
/// Allowlist, not denylist: WXSS supports only simple class/element selectors,
/// `page`, `::before`/`::after`, and comma grouping — anything else (pseudo-classes,
/// combinators, attribute/universal selectors) is rejected.
fn transform_selector(selector: &str) -> Option<String> {
    let mut result = String::new();
    let mut chars = selector.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(escaped) => result.push(sanitize_char(escaped)),
                None => {}
            }
        } else {
            result.push(c);
        }
    }
    // whatever remains unescaped is selector syntax
    for part in result.split(',') {
        if !is_simple_selector(part.trim()) {
            return None;
        }
    }
    Some(result)
}

fn is_simple_selector(part: &str) -> bool {
    let base = part
        .strip_suffix("::before")
        .or_else(|| part.strip_suffix("::after"))
        .unwrap_or(part);
    if base.is_empty() {
        return false;
    }
    let (first, name) = match base.strip_prefix('.') {
        Some(class) => (class.chars().next(), class),
        None => (base.chars().next(), base), // element selector (incl. `page`)
    };
    first.is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn unescape(selector: &str) -> String {
    selector.replace('\\', "")
}

/// `0.5rem` → `16rpx` (1rem = 32rpx)
fn convert_rem(body: &str) -> String {
    let re = Regex::new(r"(\d*\.?\d+)rem").unwrap();
    re.replace_all(body, |caps: &regex::Captures| {
        let v: f32 = caps[1].parse().unwrap_or(0.0);
        let rpx = v * 32.0;
        if rpx.fract() == 0.0 {
            format!("{}rpx", rpx as i64)
        } else {
            format!("{:.2}rpx", rpx)
        }
    })
    .to_string()
}
