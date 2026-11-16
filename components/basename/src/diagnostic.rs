use crate::{
    options::Problem,
    quoting::{Style, quote},
};
use alloc::vec::Vec;
pub fn message(problem: Problem<'_>, utf8: bool) -> Vec<u8> {
    use Problem::*;
    let enclosed = |head: &[u8], v: &[u8], tail: &[u8]| [head, v, tail].concat();
    match problem {
        Missing => b"missing operand".to_vec(),
        Extra(v) => enclosed(b"extra operand ", &quote(v, Style::Locale, utf8), b""),
        Short(v) => enclosed(b"invalid option -- '", &[v], b"'"),
        NeedSuffix(true) => b"option '--suffix' requires an argument".to_vec(),
        NeedSuffix(false) => b"option requires an argument -- 's'".to_vec(),
        Unexpected(v) => enclosed(b"option '--", v, b"' doesn't allow an argument"),
        Unknown(v) => enclosed(b"unrecognized option '", v, b"'"),
        Ambiguous(v) => enclosed(
            b"option '",
            v,
            b"' is ambiguous; possibilities: '--multiple' '--suffix' '--zero' '--help' '--version'",
        ),
    }
}
