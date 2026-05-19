//! Macro expander: single top-to-bottom pass over Normal spans only.
//! \n -> <br>. Non-normal spans passed through verbatim.
//! Returns expanded string and adjusted spans (same span count, updated positions).

use crate::{
    escape::is_escaped_at,
    prelex::{Span, SpanKind},
};

pub fn expand_macros(source: &str, spans: &[Span]) -> (String, Vec<Span>) {
    let mut out = String::with_capacity(source.len());
    let mut new_spans = Vec::with_capacity(spans.len());
    let mut out_offset = 0;

    for span in spans {
        let segment = &source[span.start..span.end];
        match span.kind {
            SpanKind::Normal => {
                let expanded = expand_normal_macros(segment);
                new_spans.push(Span {
                    start: out_offset,
                    end: out_offset + expanded.len(),
                    kind: SpanKind::Normal,
                });
                out_offset += expanded.len();
                out.push_str(&expanded);
            }
            _ => {
                new_spans.push(Span {
                    start: out_offset,
                    end: out_offset + segment.len(),
                    kind: span.kind,
                });
                out_offset += segment.len();
                out.push_str(segment);
            }
        }
    }

    (out, new_spans)
}

fn expand_normal_macros(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    let mut i = 0usize;
    while i < segment.len() {
        let rest = &segment[i..];
        if rest.starts_with("\\n") && !is_escaped_at(segment, i) {
            out.push_str("<br>");
            i += 2;
            continue;
        }
        let c = rest.chars().next().expect("i is on a char boundary");
        out.push(c);
        i += c.len_utf8();
    }
    out
}
