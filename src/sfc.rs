pub struct Sfc<'a> {
    pub frontmatter: &'a str,
    pub template: &'a str,
    pub style: Option<&'a str>,
    pub style_scoped: bool,
    pub style_global: bool,
    /// 1-based line in the source file where the frontmatter body starts
    pub frontmatter_line: usize,
    /// 1-based line in the source file where the (trimmed) template starts
    pub template_line: usize,
}

fn line_of(source: &str, byte: usize) -> usize {
    source[..byte].matches('\n').count() + 1
}

pub fn split(source: &str) -> Result<Sfc<'_>, String> {
    let lead = source.len() - source.trim_start().len();
    let rest = source.trim_start();
    let rest = rest
        .strip_prefix("---")
        .ok_or("expected frontmatter opening '---'")?;
    let fm_start = lead + 3;
    let end = rest
        .find("\n---")
        .ok_or("expected frontmatter closing '---'")?;
    let frontmatter = &rest[..end];
    let after = &rest[end + 4..];
    let after_start = fm_start + end + 4;

    let (template, style, style_scoped, style_global) = match after.find("<style") {
        Some(i) => {
            let style_block = &after[i..];
            let open_end = style_block.find('>').ok_or("malformed <style> tag")?;
            let close = style_block.find("</style>").ok_or("missing </style>")?;
            let attrs = &style_block[6..open_end];
            if !attrs.is_empty() && !attrs.starts_with(char::is_whitespace) {
                return Err("malformed <style> tag — expected <style>, <style scoped> or <style global>".to_string());
            }
            for a in attrs.split_whitespace() {
                if a != "scoped" && a != "global" {
                    return Err(format!("unknown <style> attribute '{}' — supported: scoped, global", a));
                }
            }
            let scoped = attrs.split_whitespace().any(|a| a == "scoped");
            let global = attrs.split_whitespace().any(|a| a == "global");
            if scoped && global {
                return Err("<style> cannot be both scoped and global — pick one".to_string());
            }
            (&after[..i], Some(style_block[open_end + 1..close].trim()), scoped, global)
        }
        None => (after, None, false, false),
    };
    let template_trimmed = template.trim();
    let template_lead = template.len() - template.trim_start().len();
    Ok(Sfc {
        frontmatter,
        template: template_trimmed,
        style,
        style_scoped,
        style_global,
        frontmatter_line: line_of(source, fm_start),
        template_line: line_of(source, after_start + template_lead),
    })
}
