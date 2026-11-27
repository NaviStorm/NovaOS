use super::{Error, MAX_MANIFEST, Result, bounded::Budget};
use alloc::{borrow::Cow, vec::Vec};
use toml_parser::{
    Raw, Source, Span,
    decoder::{Encoding, ScalarKind, StringBuilder},
    lexer::{Token, TokenKind},
    parser::{Event, EventKind as K, RecursionGuard, ValidateWhitespace, parse_document},
};

/// La première tranche décodée reste empruntée. Un échappement peut demander
/// plusieurs fragments, dont la longueur totale reste bornée par le lexème.
struct Text<'a, 'b> {
    value: Cow<'a, str>,
    budget: &'b mut Budget,
    limit: usize,
    failed: Option<Error>,
}
impl<'a> Text<'a, '_> {
    fn append(&mut self, s: &str) -> bool {
        if self.value.len() + s.len() > self.limit {
            self.failed = Some(Error::Limit);
            return false;
        }
        if let Cow::Borrowed(old) = self.value {
            let mut dest = match self.budget.string_capacity(self.limit) {
                Ok(v) => v,
                Err(e) => {
                    self.failed = Some(e);
                    return false;
                }
            };
            dest.push_str(old);
            self.value = Cow::Owned(dest);
        }
        if let Cow::Owned(v) = &mut self.value {
            v.push_str(s);
        }
        true
    }
}
impl<'a> StringBuilder<'a> for Text<'a, '_> {
    fn clear(&mut self) {
        self.value = Cow::Borrowed("");
    }
    fn push_str(&mut self, s: &'a str) -> bool {
        if self.value.is_empty() {
            self.value = Cow::Borrowed(s);
            true
        } else {
            self.append(s)
        }
    }
    fn push_char(&mut self, c: char) -> bool {
        self.append(c.encode_utf8(&mut [0; 4]))
    }
}
#[derive(PartialEq, Eq, Clone, Copy)]
enum Form {
    Implicit,
    Dotted,
    Declared,
    Inline,
}
pub(crate) enum Value<'a> {
    Table(Table<'a>),
    Array(Vec<Value<'a>>),
    Tables(Vec<Value<'a>>),
    Scalar(ScalarKind, Cow<'a, str>),
}
impl Value<'_> {
    // Serde 4B décodait le contenu des règles de bus futurs avant de les ignorer.
    fn validate_opaque(&self) -> bool {
        match self {
            Self::Table(t) => t.fields.iter().all(|(_, v)| v.validate_opaque()),
            Self::Array(a) | Self::Tables(a) => a.iter().all(Self::validate_opaque),
            Self::Scalar(ScalarKind::Integer(r), s) => {
                i64::from_str_radix(s, r.value()).is_ok()
                    || u64::from_str_radix(s, r.value()).is_ok()
            }
            Self::Scalar(ScalarKind::Float, s) => s
                .parse::<f64>()
                .is_ok_and(|f| !f.is_infinite() || s.contains("inf")),
            Self::Scalar(_, _) => true,
        }
    }
    pub(crate) fn check_opaque(&self) -> Result<()> {
        if self.validate_opaque() {
            Ok(())
        } else {
            Err(Error::Format)
        }
    }
}
pub(crate) struct Table<'a> {
    depth: u8,
    form: Form,
    fields: Vec<(Cow<'a, str>, Value<'a>)>,
}
impl<'a> Table<'a> {
    fn new(form: Form, depth: u8) -> Self {
        Self {
            depth,
            form,
            fields: Vec::new(),
        }
    }
    pub fn take(&mut self, key: &str) -> Result<Value<'a>> {
        self.optional(key).ok_or(Error::Format)
    }
    pub fn optional(&mut self, key: &str) -> Option<Value<'a>> {
        let i = self.fields.iter().position(|(k, _)| k == key)?;
        Some(self.fields.swap_remove(i).1)
    }
    pub fn finish(self) -> Result<()> {
        if self.fields.is_empty() {
            Ok(())
        } else {
            Err(Error::Format)
        }
    }
    fn slot(
        &mut self,
        key: Cow<'a, str>,
        create: Value<'a>,
        b: &mut Budget,
    ) -> Result<(bool, &mut Value<'a>)> {
        if let Some(i) = self.fields.iter().position(|(k, _)| *k == key) {
            return Ok((false, &mut self.fields[i].1));
        }
        b.push(&mut self.fields, (key, create))?;
        Ok((true, &mut self.fields.last_mut().ok_or(Error::Format)?.1))
    }
}
// Toutes les positions viennent du même document contrôlé à 16 Kio.
struct Mark {
    kind: K,
    encoding: Option<Encoding>,
    start: u16,
    end: u16,
}
impl From<Event> for Mark {
    fn from(e: Event) -> Self {
        Self {
            kind: e.kind(),
            encoding: e.encoding(),
            start: e.span().start() as u16,
            end: e.span().end() as u16,
        }
    }
}
struct Reader<'a, 'b> {
    src: Source<'a>,
    events: &'b [Mark],
    pos: usize,
}
impl<'a> Reader<'a, '_> {
    fn peek(&self) -> Option<K> {
        self.events.get(self.pos).map(|e| e.kind)
    }
    fn next(&mut self, k: K) -> Result<Raw<'a>> {
        let e = self
            .events
            .get(self.pos)
            .filter(|e| e.kind == k)
            .ok_or(Error::Format)?;
        self.pos += 1;
        let event = Event::new_unchecked(
            e.kind,
            e.encoding,
            Span::new_unchecked(e.start.into(), e.end.into()),
        );
        self.src.get(event).ok_or(Error::Format)
    }
    fn atom(&mut self, key: bool, b: &mut Budget) -> Result<(ScalarKind, Cow<'a, str>)> {
        let raw = self.next(if key { K::SimpleKey } else { K::Scalar })?;
        let mut out = Text {
            value: Cow::Borrowed(""),
            budget: b,
            limit: raw.len(),
            failed: None,
        };
        let mut err = None;
        let kind = if key {
            raw.decode_key(&mut out, &mut err);
            ScalarKind::String
        } else {
            raw.decode_scalar(&mut out, &mut err)
        };
        if let Some(e) = out.failed {
            return Err(e);
        }
        if err.is_some()
            || (kind == ScalarKind::DateTime
                && out.value.parse::<toml_datetime::Datetime>().is_err())
        {
            return Err(Error::Format);
        }
        Ok((kind, out.value))
    }
    fn value(&mut self, b: &mut Budget, depth: u8) -> Result<Value<'a>> {
        if depth > 32 {
            return Err(Error::Limit);
        }
        match self.peek() {
            Some(K::Scalar) => {
                let (k, s) = self.atom(false, b)?;
                Ok(Value::Scalar(k, s))
            }
            Some(K::ArrayOpen) => {
                self.next(K::ArrayOpen)?;
                let mut v = Vec::new();
                while self.peek() != Some(K::ArrayClose) {
                    let item = self.value(b, depth + 1)?;
                    b.push(&mut v, item)?;
                    if self.peek() == Some(K::ValueSep) {
                        self.next(K::ValueSep)?;
                    }
                }
                self.next(K::ArrayClose)?;
                Ok(Value::Array(v))
            }
            Some(K::InlineTableOpen) => {
                self.next(K::InlineTableOpen)?;
                let mut t = Table::new(Form::Inline, depth);
                while self.peek() != Some(K::InlineTableClose) {
                    self.assignment(&mut t, b)?;
                    if self.peek() == Some(K::ValueSep) {
                        self.next(K::ValueSep)?;
                    }
                }
                self.next(K::InlineTableClose)?;
                Ok(Value::Table(t))
            }
            _ => Err(Error::Format),
        }
    }
    fn assignment(&mut self, mut t: &mut Table<'a>, b: &mut Budget) -> Result<()> {
        loop {
            if t.depth >= 32 {
                return Err(Error::Limit);
            }
            let (_, key) = self.atom(true, b)?;
            if self.peek() == Some(K::KeySep) {
                self.next(K::KeySep)?;
                let (_, v) = t.slot(key, Value::Table(Table::new(Form::Dotted, t.depth + 1)), b)?;
                match v {
                    Value::Table(child) if child.form == Form::Dotted => t = child,
                    _ => return Err(Error::Format),
                }
            } else {
                self.next(K::KeyValSep)?;
                let v = self.value(b, t.depth + 1)?;
                if !t.slot(key, v, b)?.0 {
                    return Err(Error::Format);
                }
                return Ok(());
            }
        }
    }
    fn header<'t>(
        &mut self,
        mut t: &'t mut Table<'a>,
        b: &mut Budget,
    ) -> Result<&'t mut Table<'a>> {
        let array = self.peek() == Some(K::ArrayTableOpen);
        self.next(if array {
            K::ArrayTableOpen
        } else {
            K::StdTableOpen
        })?;
        loop {
            if t.depth >= 32 {
                return Err(Error::Limit);
            }
            let (_, key) = self.atom(true, b)?;
            if self.peek() == Some(K::KeySep) {
                self.next(K::KeySep)?;
                let (_, v) = t.slot(
                    key,
                    Value::Table(Table::new(Form::Implicit, t.depth + 1)),
                    b,
                )?;
                t = match v {
                    Value::Table(t) if t.form != Form::Inline => t,
                    Value::Tables(rows) => match rows.last_mut() {
                        Some(Value::Table(t)) => t,
                        _ => return Err(Error::Format),
                    },
                    _ => return Err(Error::Format),
                };
            } else if array {
                self.next(K::ArrayTableClose)?;
                let depth = t.depth + 1;
                let (_, v) = t.slot(key, Value::Tables(Vec::new()), b)?;
                if let Value::Tables(rows) = v {
                    b.push(rows, Value::Table(Table::new(Form::Declared, depth)))?;
                    if let Some(Value::Table(t)) = rows.last_mut() {
                        return Ok(t);
                    }
                }
                return Err(Error::Format);
            } else {
                self.next(K::StdTableClose)?;
                let (_, v) = t.slot(
                    key,
                    Value::Table(Table::new(Form::Implicit, t.depth + 1)),
                    b,
                )?;
                if let Value::Table(t) = v
                    && t.form == Form::Implicit
                {
                    t.form = Form::Declared;
                    return Ok(t);
                }
                return Err(Error::Format);
            }
        }
    }
}
// Une suite de lignes vides ou commentées a le même séparateur grammatical.
// Chaque commentaire et chaque fin de ligne supprimés sont validés au passage.
fn tokens(src: Source<'_>) -> impl Iterator<Item = Token> {
    let mut line_start = true;
    src.lex().filter(move |token| {
        let kind = token.kind();
        if line_start
            && matches!(
                kind,
                TokenKind::Newline | TokenKind::Whitespace | TokenKind::Comment
            )
        {
            let mut err = None;
            if let Some(raw) = src.get(token) {
                match kind {
                    TokenKind::Newline => raw.decode_newline(&mut err),
                    TokenKind::Comment => raw.decode_comment(&mut err),
                    _ => {}
                }
                if err.is_none() {
                    return false;
                }
            }
        }
        line_start = kind == TokenKind::Newline;
        true
    })
}
pub(crate) fn parse<'a>(raw: &'a [u8], b: &mut Budget) -> Result<Table<'a>> {
    if raw.len() > MAX_MANIFEST {
        return Err(Error::Limit);
    }
    let src = Source::new(core::str::from_utf8(raw).map_err(|_| Error::Format)?);
    let mut lexed = Vec::new();
    b.reserve(&mut lexed, tokens(src).count())?;
    lexed.extend(tokens(src));
    let mut events = Vec::new();
    let mut failure = None;
    let mut emit = |e: Event| {
        if failure.is_none()
            && !matches!(e.kind(), K::Whitespace | K::Comment | K::Newline)
            && let Err(reason) = b.push(&mut events, e.into())
        {
            failure = Some(reason);
        }
    };
    let mut err = None;
    parse_document(
        &lexed,
        &mut RecursionGuard::new(&mut ValidateWhitespace::new(&mut emit, src), 32),
        &mut err,
    );
    if let Some(reason) = failure {
        return Err(reason);
    }
    if err.is_some() {
        return Err(Error::Format);
    }
    drop(lexed);
    let mut reader = Reader {
        src,
        events: &events,
        pos: 0,
    };
    let mut root = Table::new(Form::Declared, 0);
    while reader.peek() == Some(K::SimpleKey) {
        reader.assignment(&mut root, b)?;
    }
    while reader.peek().is_some() {
        let table = reader.header(&mut root, b)?;
        while reader.peek() == Some(K::SimpleKey) {
            reader.assignment(table, b)?;
        }
    }
    Ok(root)
}
