//! then post-process the CSS for WXSS — selector sanitization (matching the markup
//! selectors WXSS cannot express (reported, never silently dropped).

use crate::tailwind::sanitize_char;
use regex::Regex;
use std::fs;
use std::process::Command;

pub struct CliCss {
    pub css: String,
    /// `page { ... }` theme-variable rules — legal only in page/app WXSS, so they
    pub theme_css: String,
    /// rules removed because WXSS cannot express their selector
    pub dropped_selectors: Vec<String>,
}

pub fn generate(classes: &[String]) -> Result<CliCss, String> {
    let raw = run_cli_v4(classes)?;
    Ok(postprocess_v4(&raw))
}

fn v4_cache_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("mistc-tw4")
}

fn run_cli_v4(classes: &[String]) -> Result<String, String> {
    let dir = v4_cache_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if !dir.join("node_modules/@tailwindcss/cli").exists() {
        fs::write(dir.join("package.json"), "{ \"name\": \"mistc-tw\", \"private\": true }")
            .map_err(|e| e.to_string())?;
        let out = Command::new("npm")
            .args(["install", "--no-audit", "--no-fund", "tailwindcss@^4", "@tailwindcss/cli@^4"])
            .current_dir(&dir)
            .output()
            .map_err(|e| format!("npm not available: {}", e))?;
        if !out.status.success() {
            return Err(format!("npm install failed: {}", String::from_utf8_lossy(&out.stderr)));
        }
    }
    // per-invocation IO dir — concurrent builds must not interleave files
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let io = dir.join(format!(
        "io-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir_all(&io).map_err(|e| e.to_string())?;
    fs::write(io.join("content.html"), format!("<div class=\"{}\"></div>", classes.join(" ")))
        .map_err(|e| e.to_string())?;
    fs::write(
        io.join("input.css"),
        "@import \"tailwindcss/theme.css\" layer(theme);\n@import \"tailwindcss/utilities.css\" layer(utilities);\n@source \"./content.html\";\n",
    )
    .map_err(|e| e.to_string())?;
    let input = io.join("input.css");
    let output = io.join("out.css");
    let out = Command::new("npx")
        .arg("@tailwindcss/cli")
        .arg("-i")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .current_dir(&dir)
        .output()
        .map_err(|e| format!("npx failed to start: {}", e))?;
    if !out.status.success() {
        return Err(format!("@tailwindcss/cli failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let css = fs::read_to_string(&output).map_err(|e| e.to_string());
    let _ = fs::remove_dir_all(&io);
    css
}

/// v4 output → WXSS: unwrap `@layer`, `:root,:host` → `page`, resolve
pub fn postprocess_v4(css: &str) -> CliCss {
    let comments = Regex::new(r"(?s)/\*.*?\*/").unwrap();
    let css = comments.replace_all(css, "").to_string();

    let (css, initials) = extract_property_initials(&css);
    let css = css.replace("@layer properties;", "");
    let css = unwrap_layers(&css);
    let css = css.replace(":root, :host", "page");
    let css = substitute_tw_vars(&css, &initials);
    let css = convert_oklch(&css);
    let css = convert_color_mix(&css);
    let css = convert_media_ranges(&css);
    // v4 `rounded-full`; WXSS can't parse infinity
    let css = css.replace("calc(infinity * 1px)", "9999px");

    let mut out = String::new();
    let mut dropped = Vec::new();
    process_block(&css, &mut out, &mut dropped, "");
    let (css, theme_css) = split_theme(&out);
    CliCss { css, theme_css, dropped_selectors: dropped }
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

fn extract_property_initials(css: &str) -> (String, std::collections::BTreeMap<String, String>) {
    let mut map = std::collections::BTreeMap::new();
    let re = Regex::new(r"(?s)@property\s+(--[\w-]+)\s*\{[^}]*?initial-value:\s*([^;}]+);?[^}]*\}").unwrap();
    for caps in re.captures_iter(css) {
        map.insert(caps[1].to_string(), caps[2].trim().to_string());
    }
    let stripped = Regex::new(r"(?s)@property\s+--[\w-]+\s*\{[^}]*\}").unwrap();
    (stripped.replace_all(css, "").to_string(), map)
}

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

fn substitute_tw_vars(css: &str, initials: &std::collections::BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(pos) = rest.find("var(--tw-") {
        out.push_str(&rest[..pos]);
        let open = pos + 3;
        let close = matching_paren(&rest[open..]);
        let inner = &rest[open + 1..open + close];
        let (name, fallback) = match top_level_comma(inner) {
            Some(i) => (inner[..i].trim(), Some(inner[i + 1..].trim())),
            None => (inner.trim(), None),
        };
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

pub fn postprocess(css: &str) -> CliCss {
    let comments = Regex::new(r"(?s)/\*.*?\*/").unwrap();
    let css = comments.replace_all(css, "");
    let mut out = String::new();
    let mut dropped = Vec::new();
    process_block(&css, &mut out, &mut dropped, "");
    let (css, theme_css) = split_theme(&out);
    CliCss { css, theme_css, dropped_selectors: dropped }
}

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
                let body = match body.find('@') {
                    Some(at) => body[..at].trim_end().trim_end_matches(';'),
                    None => body.trim_end(),
                };
                let mut body = convert_rem(body);
                // preflight (unavailable in WXSS — needs `*`) normally supplies
                if body.contains("border-width") && !body.contains("border-style") {
                    body.push_str(";\n  border-style: solid;\n  border-color: #e5e7eb");
                }
                out.push_str(&format!("{}{} {{{} }}\n", indent, selector, body));
            }
            None => {
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
/// Allowlist, not denylist: WXSS supports only simple class/element selectors,
fn transform_selector(selector: &str) -> Option<String> {
    let mut result = String::new();
    let mut chars = selector.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(escaped) = chars.next() { result.push(sanitize_char(escaped)) }
        } else {
            result.push(c);
        }
    }
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
        None => (base.chars().next(), base),
    };
    first.is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn unescape(selector: &str) -> String {
    selector.replace('\\', "")
}

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
