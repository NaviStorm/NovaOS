use super::{Flags, Key};
use crate::args::Options;
use alloc::{format, string::String, vec::Vec};

impl Flags {
    fn enabled(self, c: u8) -> bool {
        match c {
            b'b' => self.blank,
            b'd' => self.dictionary,
            b'f' => self.fold,
            b'i' => self.printable,
            b'r' => self.reverse,
            _ => b"ghMnRV"
                .iter()
                .position(|&v| v == c)
                .is_some_and(|i| self.modes & (1 << i) != 0),
        }
    }
}

impl Key {
    fn blank_edges(&self, global: Flags) -> (bool, bool) {
        if self.local {
            (self.start.blank, self.end.is_some_and(|e| e.blank))
        } else {
            (global.blank, global.blank)
        }
    }
}

pub fn warnings(o: &Options) -> Vec<String> {
    let mut output = Vec::new();
    let mut numeric = false;
    let mut span_basic = false;
    let mut span_general = false;
    let mut used = Vec::new();
    let (mut leading, mut trailing) = (false, false);
    for i in 0..o.keys.len().max(1) {
        let key = o.keys.get(i);
        let f = key.filter(|k| k.local).map_or(o.flags, |k| k.flags);
        used.push(f);
        let number = matches!(f.mode, b'g' | b'h' | b'n');
        numeric |= number;
        let Some(k) = key else { continue };
        let (left, right) = k.blank_edges(o.flags);
        leading |= left;
        trailing |= right;
        let n = i + 1;
        if k.obsolete {
            let mut old = format!("+{}", k.start.field);
            let mut new = format!("-k {}", k.start.field.saturating_add(1));
            if let Some(end) = k.end {
                let field = end.field.saturating_add(1);
                old.push_str(&format!(" -{field}"));
                new.push_str(&format!(",{field}"));
            }
            output.push(format!(
                "obsolescent key '{old}' used; consider '{new}' instead"
            ));
        }
        let empty = k.empty_end || k.end.is_some_and(|e| e.field < k.start.field);
        if empty {
            output.push(format!("key {n} has zero width and will be ignored"));
        }
        let whole_line_offset = k.end.is_some_and(|e| e.field == 0 && e.column != 0);
        let significant = (!left && (!number && f.mode != b'M' || k.start.column > 1))
            || (!right && k.end.is_some_and(|e| e.column > 0));
        if !empty && o.sep.is_none() && !whole_line_offset && significant {
            output.push(format!(
                "leading blanks are significant in key {n}; consider also specifying 'b'"
            ));
        }
        if number && k.spans_fields() {
            span_general |= f.mode == b'g';
            span_basic |= f.mode != b'g';
            output.push(format!("key {n} is numeric and spans multiple fields"));
        }
    }
    if numeric {
        let point = o.decimal.unwrap_or(b'.');
        let num_sep = if span_basic && o.sep.is_some() && o.sep == o.thousands {
            Some("a group separator")
        } else if (span_basic || span_general) && o.sep == Some(point) {
            Some("a decimal point")
        } else {
            None
        };
        if let (Some(sep), Some(role)) = (o.sep, num_sep) {
            output.push(format!(
                "field separator '{}' is treated as {role} in numbers",
                sep as char
            ));
        } else {
            if (span_basic || span_general) && o.sep == Some(b'-') {
                output.push("field separator '-' is treated as a minus sign in numbers".into());
            } else if span_general && o.sep == Some(b'+') {
                output.push("field separator '+' is treated as a plus sign in numbers".into());
            }
            output.push(format!(
                "numbers use '{}' as a decimal point in this locale",
                point as char
            ));
        }
    }
    if !o.keys.is_empty() {
        let mut ignored = String::new();
        let mut fallback = false;
        for &c in b"bdfghiMnRrV" {
            if !o.flags.enabled(c) {
                continue;
            }
            let applied = if c == b'b' {
                leading && trailing
            } else {
                used.iter().any(|f| f.enabled(c))
            };
            if !applied {
                if c == b'r' && !o.unique && !o.stable {
                    fallback = true;
                } else {
                    ignored.push(c as char);
                }
            }
        }
        if !ignored.is_empty() {
            output.push(if ignored.len() == 1 {
                format!("option '-{ignored}' is ignored")
            } else {
                format!("options '-{ignored}' are ignored")
            });
        }
        if fallback {
            output.push("option '-r' only applies to last-resort comparison".into());
        }
    }
    output
}
