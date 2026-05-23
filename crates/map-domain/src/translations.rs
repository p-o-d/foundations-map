//! Shared helper for resolving X4 translation refs into human display names.
//!
//! `x4_display_name` applies X4's display conventions uniformly:
//!   - Text starting with `(NAME)`: NAME is the display (translator-asserted
//!     composite name); the rest is X4 internal composition.
//!   - Text starting with `{p,t}` or plain text: substitute each `{p,t}`
//!     recursively via the same function (so sub-refs strip their own
//!     trailing translator parens), then strip any trailing unescaped
//!     `(description)` from the result.
//!   - `\(` and `\)` escape sequences are unescaped in the final output.

use std::collections::HashMap;

const MAX_RECURSION_DEPTH: u32 = 6;

/// Resolve an X4 translation entry into a clean human display name.
/// `translations` maps `(page_id, text_id)` → raw entry text.
pub fn x4_display_name(text: &str, translations: &HashMap<(u32, u32), String>) -> String {
    x4_display_name_inner(text, translations, 0)
}

fn x4_display_name_inner(
    text: &str,
    translations: &HashMap<(u32, u32), String>,
    depth: u32,
) -> String {
    let trimmed = text.trim();

    if let Some(rest) = trimmed.strip_prefix('(') {
        if let Some(close) = find_unescaped(rest, ')') {
            return unescape_parens(rest[..close].trim());
        }
    }

    let substituted = if depth >= MAX_RECURSION_DEPTH {
        trimmed.to_string()
    } else {
        substitute_refs(trimmed, translations, depth)
    };

    let stripped = strip_trailing_paren(substituted.trim());
    unescape_parens(stripped.trim())
}

/// Substitute each `{page,id}` substring with the recursive display of the
/// referenced entry. Unknown refs and malformed brace groups stay verbatim.
fn substitute_refs(
    s: &str,
    translations: &HashMap<(u32, u32), String>,
    depth: u32,
) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        match rest.find('{') {
            None => {
                out.push_str(rest);
                break;
            }
            Some(open) => {
                out.push_str(&rest[..open]);
                let after_open = &rest[open + 1..];
                match after_open.find('}') {
                    None => {
                        out.push('{');
                        rest = after_open;
                    }
                    Some(close_rel) => {
                        let inner = &after_open[..close_rel];
                        let parsed: Option<(u32, u32)> = inner
                            .split_once(',')
                            .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)));
                        match parsed.and_then(|(p, t)| translations.get(&(p, t))) {
                            Some(referenced) => {
                                let resolved = x4_display_name_inner(referenced, translations, depth + 1);
                                out.push_str(&resolved);
                            }
                            None => {
                                out.push('{');
                                out.push_str(inner);
                                out.push('}');
                            }
                        }
                        rest = &after_open[close_rel + 1..];
                    }
                }
            }
        }
    }
    out
}

/// Find the first UNESCAPED occurrence of `target` byte in `text`. A `target`
/// char preceded by an odd number of backslashes is considered escaped.
fn find_unescaped(text: &str, target: char) -> Option<usize> {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b as char != target {
            continue;
        }
        let mut backslashes = 0;
        let mut j = i;
        while j > 0 && bytes[j - 1] == b'\\' {
            backslashes += 1;
            j -= 1;
        }
        if backslashes % 2 == 0 {
            return Some(i);
        }
    }
    None
}

/// Strip a single trailing unescaped `(...)` from `s` if present.
/// Returns the substring before the opening unescaped `(` of the last
/// top-level paren group. If no trailing paren exists, returns `s` unchanged.
fn strip_trailing_paren(s: &str) -> String {
    let s = s.trim_end();
    if !s.ends_with(')') {
        return s.to_string();
    }
    // Find the matching unescaped `(`. We walk from the end and balance.
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        let c = bytes[i] as char;
        if c != '(' && c != ')' {
            continue;
        }
        // Check if escaped (odd backslashes before it).
        let mut backslashes = 0;
        let mut j = i;
        while j > 0 && bytes[j - 1] == b'\\' {
            backslashes += 1;
            j -= 1;
        }
        if backslashes % 2 == 1 {
            continue;
        }
        if c == ')' {
            depth += 1;
        } else {
            depth -= 1;
            if depth == 0 {
                // i is the opening `(`. Return everything before it.
                return s[..i].trim_end().to_string();
            }
        }
    }
    // Unbalanced; leave as-is.
    s.to_string()
}

/// Unescape `\(` → `(` and `\)` → `)`. Other backslash sequences pass through
/// verbatim (the backslash is preserved).
fn unescape_parens(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some(&'(') | Some(&')') => {
                    out.push(chars.next().unwrap());
                }
                _ => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ship_translations() -> HashMap<(u32, u32), String> {
        let mut m = HashMap::new();
        // Plain class name.
        m.insert((20101, 10101), "Discoverer".into());
        // Ship with trailing description.
        m.insert((20101, 122701), "Wayfinder(ALI Expedition ship)".into());
        // Variant with leading paren.
        m.insert((20101, 30801), "Helios".into());
        m.insert((20111, 5462), "E".into());
        m.insert((20101, 30804), "(Helios E){20101,30801} {20111,5462}".into());
        // Variant with escaped parens.
        m.insert((20101, 10801), "Drill".into());
        m.insert((20111, 3101), "(Mineral)".into());
        m.insert((20111, 1101), "Vanguard".into());
        // We test the leading-paren form for this case explicitly below.
        m
    }

    fn sector_translations() -> HashMap<(u32, u32), String> {
        let mut m = HashMap::new();
        // Cluster name source (page 20005).
        m.insert((20005, 6002), "Большая биржа".into());
        m.insert((20005, 6055), "Пустошь возможностей".into());
        // Numeral page.
        m.insert((20402, 1), "I(speak as 1)".into());
        // Cluster page entry with composite ref + paren hint.
        m.insert((20003, 10001), "{20005,6002}(Большая биржа)".into());
        // Sector page entries with cluster ref + numeral ref + paren hint.
        m.insert((20004, 10011), "{20003,10001} {20402,1}(Большая Биржа I)".into());
        // Cluster page entry that ONLY uses ref substitution (paren is translator-only).
        m.insert((20003, 7250001), "{20005,6055}(Void of Opportunity)".into());
        m
    }

    #[test]
    fn plain_text_returned_as_is() {
        let t = ship_translations();
        assert_eq!(x4_display_name("Discoverer", &t), "Discoverer");
    }

    #[test]
    fn empty_returns_empty() {
        let t = HashMap::new();
        assert_eq!(x4_display_name("", &t), "");
    }

    #[test]
    fn leading_paren_returns_paren_contents() {
        let t = ship_translations();
        assert_eq!(
            x4_display_name("(Helios E){20101,30801} {20111,5462}", &t),
            "Helios E"
        );
    }

    #[test]
    fn leading_paren_with_escaped_inner_parens_unescaped() {
        let t = ship_translations();
        assert_eq!(
            x4_display_name("(Drill \\(Mineral\\) Vanguard){20101,10801}", &t),
            "Drill (Mineral) Vanguard"
        );
    }

    #[test]
    fn trailing_paren_is_treated_as_description() {
        let t = ship_translations();
        assert_eq!(
            x4_display_name("Wayfinder(ALI Expedition ship)", &t),
            "Wayfinder"
        );
    }

    #[test]
    fn trailing_paren_with_escaped_parens_in_name_unescaped() {
        let t = HashMap::new();
        assert_eq!(
            x4_display_name("Drill \\(Mineral\\) Mk1(description)", &t),
            "Drill (Mineral) Mk1"
        );
    }

    #[test]
    fn ref_substitution_resolves_sub_refs_recursively() {
        let t = ship_translations();
        // {20101,30804} → "(Helios E){20101,...} {20111,...}" → leading paren → "Helios E"
        assert_eq!(x4_display_name("{20101,30804}", &t), "Helios E");
    }

    #[test]
    fn refs_substituted_then_trailing_paren_stripped() {
        // The Void of Opportunity case: trailing paren is translator hint.
        let t = sector_translations();
        assert_eq!(
            x4_display_name("{20005,6055}(Void of Opportunity)", &t),
            "Пустошь возможностей"
        );
    }

    #[test]
    fn composite_sector_entry_resolves_via_recursive_substitution() {
        // Each sub-ref is itself resolved through x4_display_name, so its
        // trailing translator paren is stripped before interpolation.
        let t = sector_translations();
        assert_eq!(
            x4_display_name("{20003,10001} {20402,1}(Большая Биржа I)", &t),
            "Большая биржа I"
        );
    }

    #[test]
    fn cluster_entry_via_ref_extracts_correctly() {
        let t = sector_translations();
        // Lookup of cluster 10001 directly.
        assert_eq!(
            x4_display_name("{20003,10001}", &t),
            "Большая биржа"
        );
    }

    #[test]
    fn unknown_ref_left_intact_inside_result() {
        let mut t = HashMap::new();
        t.insert((20101, 1), "Foo".into());
        // {99999,1} stays verbatim; visible for debugging.
        assert_eq!(
            x4_display_name("{20101,1} {99999,1}", &t),
            "Foo {99999,1}"
        );
    }

    #[test]
    fn self_referential_loop_terminates() {
        let mut t = HashMap::new();
        t.insert((1, 1), "{1,1}".into());
        let result = x4_display_name("{1,1}", &t);
        // Just need termination + bounded size.
        assert!(result.len() < 10_000);
    }

    #[test]
    fn malformed_brace_passes_through() {
        let t = HashMap::new();
        assert_eq!(x4_display_name("{not,a,ref}", &t), "{not,a,ref}");
        assert_eq!(x4_display_name("{", &t), "{");
    }
}
