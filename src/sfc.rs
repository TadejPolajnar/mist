pub struct Sfc<'a> {
    pub frontmatter: &'a str,
    pub template: &'a str,
    pub style: Option<&'a str>,
    pub style_scoped: bool,
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

    let (template, style, style_scoped) = match after.find("<style") {
        Some(i) => {
            let style_block = &after[i..];
            let open_end = style_block.find('>').ok_or("malformed <style> tag")?;
            let close = style_block.find("</style>").ok_or("missing </style>")?;
            let attrs = &style_block[6..open_end];
            let scoped = attrs.split_whitespace().any(|a| a == "scoped");
            (&after[..i], Some(style_block[open_end + 1..close].trim()), scoped)
        }
        None => (after, None, false),
    };
    let template_trimmed = template.trim();
    let template_lead = template.len() - template.trim_start().len();
    Ok(Sfc {
        frontmatter,
        template: template_trimmed,
        style,
        style_scoped,
        frontmatter_line: line_of(source, fm_start),
        template_line: line_of(source, after_start + template_lead),
    })
}
