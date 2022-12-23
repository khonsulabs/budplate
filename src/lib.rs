#![doc = include_str!("../README.md")]

use std::{borrow::Cow, collections::HashMap, fmt::Write, ops::Range};

use budlang::{
    vm::{ir::Function, Destination, FaultKind, Instruction, NativeFunction, Symbol, Value},
    Bud,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Template<'a> {
    source: Cow<'a, str>,
}

impl<'a> Template<'a> {
    pub const fn from_str(template: &'a str) -> Self {
        Self {
            source: Cow::Borrowed(template),
        }
    }

    pub const fn from_string(template: String) -> Self {
        Self {
            source: Cow::Owned(template),
        }
    }

    pub fn render(&self) -> Result<String, Error> {
        self.render_with::<&'static str, Value, _>([])
    }

    pub fn render_with<Name, Arg, Args>(&self, args: Args) -> Result<String, Error>
    where
        Args: IntoIterator<Item = (Name, Arg)>,
        Name: Into<Symbol>,
        Arg: Into<Value>,
    {
        Configuration::default().render_with(&self.source, args)
    }

    fn parse(&self) -> Result<ParsedTemplate<'_>, Error> {
        enum CodeKind {
            SafeExpression,
            UnsafeExpression,
            Statement,
        }
        let mut segments = Vec::new();
        let source: &str = &self.source;
        let mut parts = source.split("{{");
        if let Some(raw_start) = parts.next() {
            let offset = raw_start.as_ptr() as usize - source.as_ptr() as usize;

            segments.push(Segment {
                kind: SegmentKind::Raw,
                range: offset..offset + raw_start.len(),
            });

            for after_brace_start in parts {
                let mut command_parts = after_brace_start.split("}}");
                let command = command_parts.next().ok_or(Error::MissingEndBraces)?;

                let (code_kind, command) = if let Some(command) = command.strip_prefix('=') {
                    (CodeKind::UnsafeExpression, command)
                } else if let Some(command) = command.strip_prefix(":=") {
                    (CodeKind::SafeExpression, command)
                } else {
                    (CodeKind::Statement, command)
                };

                let (trim_before, command) = if let Some(command) = command.strip_prefix('-') {
                    (true, command)
                } else {
                    (false, command)
                };

                let (trim_after, command) = if let Some(command) = command.strip_suffix('-') {
                    (true, command)
                } else {
                    (false, command)
                };

                let trimming = WhitespaceTrimming {
                    trim_before,
                    trim_after,
                };

                let kind = match code_kind {
                    CodeKind::SafeExpression => SegmentKind::Expression {
                        trimming,
                        safe: true,
                    },
                    CodeKind::UnsafeExpression => SegmentKind::Expression {
                        trimming,
                        safe: false,
                    },
                    CodeKind::Statement => SegmentKind::Statement(trimming),
                };

                let offset = command.as_ptr() as usize - source.as_ptr() as usize;
                segments.push(Segment {
                    kind,
                    range: offset..offset + command.len(),
                });

                if let Some(raw_end) = command_parts.next() {
                    let offset = raw_end.as_ptr() as usize - source.as_ptr() as usize;
                    segments.push(Segment {
                        kind: SegmentKind::Raw,
                        range: offset..offset + raw_end.len(),
                    });

                    if command_parts.next().is_some() {
                        return Err(Error::UnexpectedEndBrances);
                    }
                }
            }
        }

        Ok(ParsedTemplate { source, segments })
    }
}

impl<'a> From<&'a str> for Template<'a> {
    fn from(tpl: &'a str) -> Self {
        Self::from_str(tpl)
    }
}

impl<'a> From<String> for Template<'a> {
    fn from(tpl: String) -> Self {
        Self::from_string(tpl)
    }
}

#[derive(Debug, Clone)]
struct Segment {
    kind: SegmentKind,
    range: Range<usize>,
}

#[derive(Debug, Clone, Copy)]
enum SegmentKind {
    Raw,
    Statement(WhitespaceTrimming),
    Expression {
        trimming: WhitespaceTrimming,
        safe: bool,
    },
}

#[derive(Debug)]
pub enum Error {
    MissingEndBraces,
    UnexpectedEndBrances,
}

#[derive(Debug, Clone, Copy)]
pub struct WhitespaceTrimming {
    pub trim_before: bool,
    pub trim_after: bool,
}

#[derive(Debug)]
struct ParsedTemplate<'a> {
    source: &'a str,
    segments: Vec<Segment>,
}

impl<'a> ParsedTemplate<'a> {
    pub fn to_bud_source(&self, name: &str, parameters: &[Symbol]) -> String {
        let mut segments = self.segments.iter().cloned().peekable();
        let mut source = String::with_capacity(self.source.len());
        source.push_str("function ");
        source.push_str(name);
        source.push('(');
        for (index, param) in parameters.iter().enumerate() {
            if index > 0 {
                source.push_str(", ");
            }
            source.push_str(param);
        }
        source.push_str(")\noutput := \"\"\n");
        let mut trim_next_start = false;
        let mut is_at_line_start = true;

        while let Some(segment) = segments.next() {
            match segment.kind {
                SegmentKind::Raw => {
                    if segment.range.is_empty() {
                        continue;
                    }
                    // Render this as a string literal
                    if is_at_line_start {
                        is_at_line_start = false;
                        source.push_str("output := output + ");
                    } else {
                        source.push_str(" + ");
                    }
                    let mut literal = &self.source[segment.range];
                    if trim_next_start {
                        literal = literal.trim_start();
                    }
                    if matches!(segments.peek(), Some(Segment{ kind: SegmentKind::Statement(trimming) | SegmentKind::Expression{ trimming, ..}, .. }) if trimming.trim_before)
                    {
                        literal = literal.trim_end();
                    }
                    write!(
                        &mut source,
                        "{}",
                        budlang::vm::StringLiteralDisplay::new(literal)
                    )
                    .expect("failed to display literal");
                }
                SegmentKind::Statement(trimming) => {
                    trim_next_start = trimming.trim_after;
                    // A statement that stands on its own line.
                    if !is_at_line_start {
                        source.push('\n');
                        is_at_line_start = true;
                    }
                    let statement = self.source[segment.range].trim();
                    writeln!(&mut source, "{statement}").expect("failed to render statement");
                }
                SegmentKind::Expression { trimming, safe } => {
                    trim_next_start = trimming.trim_after;
                    // An inline Bud expression
                    if is_at_line_start {
                        is_at_line_start = false;
                        source.push_str("output := output + ");
                    } else {
                        source.push_str(" + ");
                    }

                    let expression = self.source[segment.range].trim();
                    if safe {
                        write!(&mut source, "(({expression}) as String)")
                            .expect("failed to render expression");
                    } else {
                        write!(&mut source, "encode(({expression}) as String)")
                            .expect("failed to render expression");
                    }
                }
            }
        }
        source.push_str("\noutput\nend");

        println!("{source}");

        source
    }
}

#[test]
fn hello_world_to_bud() {
    let template = Template::from("Hello, {{= name }}!");
    let rendered = template
        .render_with([(Symbol::from("name"), Value::from("World"))])
        .unwrap();

    assert_eq!(rendered, "Hello, World!");
}

#[test]
fn trim_tests() {
    assert_eq!(Template::from(r#" {{= "a" }} "#).render().unwrap(), " a ");
    assert_eq!(Template::from(r#" {{=- "a" -}} "#).render().unwrap(), "a");
    assert_eq!(Template::from(r#" {{=- "a" }} "#).render().unwrap(), "a ");
    assert_eq!(Template::from(r#" {{= "a" -}} "#).render().unwrap(), " a");
    assert_eq!(
        Template::from(
            r#"
                {{- if true -}}
                    {{= "a" -}}
                {{ end -}}
            "#
        )
        .render()
        .unwrap(),
        "a"
    );
}

#[test]
fn loop_test() {
    let template = Template::from("{{ loop for i := 1 to 5 inclusive }}{{= i }}{{ end }}");
    let rendered = template.render().unwrap();

    assert_eq!(rendered, "12345");
}

pub struct CompiledTemplate(Function<budlang::Intrinsic>);

pub trait Encoder: Clone + 'static {
    fn encode<W: Write>(&self, input: &str, output: &mut W);
}

#[derive(Debug, Clone)]
pub struct NoEncoding;

impl Encoder for NoEncoding {
    fn encode<W: Write>(&self, input: &str, output: &mut W) {
        output.write_str(input).unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct HtmlEncoding;

impl Encoder for HtmlEncoding {
    fn encode<W: Write>(&self, input: &str, output: &mut W) {
        let mut last_byte_written = 0;
        for (index, ch) in input.char_indices() {
            let encoded = match ch {
                '&' => "&amp;",
                '<' => "&lt;",
                '>' => "&gt;",
                '"' => "&quot;",
                '\'' => "&#39;",
                _ => continue,
            };
            if last_byte_written < index {
                output.write_str(&input[last_byte_written..index]).unwrap();
            }
            output.write_str(encoded).unwrap();
            last_byte_written = index + 1;
        }

        if last_byte_written < input.len() {
            output.write_str(&input[last_byte_written..]).unwrap();
        }
    }
}

#[test]
fn html_encoding_test() {
    let mut encoded = String::new();
    HtmlEncoding.encode("&<>'\"unencoded", &mut encoded);
    assert_eq!(encoded, "&amp;&lt;&gt;&#39;&quot;unencoded");
}

pub struct Configuration<Enc> {
    pub encoder: Enc,
    pub auto_trim: bool,
}

impl Default for Configuration<NoEncoding> {
    fn default() -> Self {
        Self {
            encoder: NoEncoding,
            auto_trim: Default::default(),
        }
    }
}

impl Configuration<HtmlEncoding> {
    pub const fn for_html() -> Self {
        Self {
            encoder: HtmlEncoding,
            auto_trim: false,
        }
    }
}

impl<Enc> Configuration<Enc>
where
    Enc: Encoder,
{
    pub fn auto_trim(mut self) -> Self {
        self.auto_trim = true;
        self
    }

    pub fn with_encoder<NewEnc>(self, encoder: NewEnc) -> Configuration<NewEnc> {
        let Self { auto_trim, .. } = self;
        Configuration { encoder, auto_trim }
    }

    pub fn render(&self, template: &str) -> Result<String, Error> {
        self.render_with::<&'static str, Value, _>(template, [])
    }

    pub fn render_with<Name, Arg, Args>(&self, template: &str, args: Args) -> Result<String, Error>
    where
        Args: IntoIterator<Item = (Name, Arg)>,
        Name: Into<Symbol>,
        Arg: Into<Value>,
    {
        let template = Template::from(template);
        let template = template.parse()?;
        let args = args.into_iter();
        let (symbols, values): (Vec<_>, Vec<_>) =
            args.map(|(name, arg)| (name.into(), arg.into())).unzip();
        let bud_source = template.to_bud_source("render", &symbols);

        let mut bud =
            Bud::empty().with_native_function("encode", EncodeFunction(self.encoder.clone()));
        bud.evaluate::<()>(&bud_source).unwrap();

        // Push
        let arg_count = values.len();
        bud.stack.extend(values).unwrap();

        Ok(bud
            .run(
                &[Instruction::Call {
                    vtable_index: Some(1),
                    arg_count,
                    destination: Destination::Return,
                }],
                0,
            )
            .unwrap())
    }
}

struct EncodeFunction<Enc>(Enc);

impl<Enc> NativeFunction for EncodeFunction<Enc>
where
    Enc: Encoder,
{
    fn invoke(&self, args: &mut budlang::vm::PoppedValues<'_>) -> Result<Value, FaultKind> {
        let arg = args
            .next()
            .ok_or_else(|| FaultKind::ArgumentMissing(Symbol::from("value")))?;
        args.verify_empty()?;

        let as_string = arg.try_convert_to_string(&())?;
        let mut encoded = String::with_capacity(as_string.len());
        self.0.encode(&as_string, &mut encoded);
        Ok(Value::from(encoded))
    }

    fn as_ptr(&self) -> *const u8 {
        self as *const Self as *const u8
    }
}

#[test]
fn html_escaped_template() {
    assert_eq!(
        Configuration::for_html()
            .render(r#"{{:= "unsafe & not encoded" }}/{{= "safe & encoded" }}"#)
            .unwrap(),
        "unsafe & not encoded/safe &amp; encoded"
    );
}
