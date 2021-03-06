use crate::class::CharClass;

use std::iter::Peekable;
use std::marker::PhantomData;
use std::str::CharIndices;

/// Alias for [`Result`] for [`ParseError`].
pub type ParseResult<'r, T> = std::result::Result<T, ParseError<'r>>;

#[derive(Debug)]
pub struct Parser<E>
where
    E: ParserEngine,
{
    _phantom: PhantomData<E>,
}

impl<E> Parser<E>
where
    E: ParserEngine,
{
    #[inline]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn parse<'r>(&self, expr: &'r str) -> ParseResult<'r, E::Output> {
        let mut state: ParserState<E> = ParserState::new();
        state.parse(expr)
    }
}

#[derive(Debug)]
pub struct ParserState<E>
where
    E: ParserEngine,
{
    engine: E,
}

pub trait ParserEngine {
    type Output;

    fn new() -> Self;

    fn handle_char<C>(&mut self, c: C) -> Self::Output
    where
        C: Into<CharClass>;
    fn handle_wildcard(&mut self) -> Self::Output;

    fn handle_star(&mut self, lhs: Self::Output) -> Self::Output;
    fn handle_plus(&mut self, lhs: Self::Output) -> Self::Output;
    fn handle_optional(&mut self, lhs: Self::Output) -> Self::Output;
    fn handle_concat(&mut self, lhs: Self::Output, rhs: Self::Output) -> Self::Output;
    fn handle_alternate(&mut self, lhs: Self::Output, rhs: Self::Output) -> Self::Output;
}

impl<E> ParserState<E>
where
    E: ParserEngine,
{
    const EXPR_START_EXPECTED: &'static [char] = &['(', '['];

    #[inline]
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { engine: E::new() }
    }

    /// Compile a regular expresion.
    #[inline]
    pub fn parse<'r>(&mut self, expr: &'r str) -> ParseResult<'r, E::Output> {
        let input = &mut ParseInput::new(expr);
        self.parse_expr(input, 0, false)
    }

    #[inline]
    fn parse_expr<'r>(
        &mut self,
        input: &mut ParseInput<'r>,
        min_bp: u8,
        parenthesized: bool,
    ) -> ParseResult<'r, E::Output> {
        let mut lhs = None;
        while lhs.is_none() {
            lhs = match input.peek() {
                Some((_, c)) => match c {
                    '\\' => Some(self.parse_escaped(input)?),
                    // Beginning of a group.
                    '(' => self.parse_group(input)?,
                    ')' if !parenthesized => {
                        let (_, c) = input.next_unchecked();
                        return Err(ParseError::UnexpectedToken {
                            span: input.current_span(),
                            token: c,
                            expected: Self::EXPR_START_EXPECTED.into(),
                        });
                    }
                    '[' => self.parse_class(input)?,
                    '.' => Some(self.parse_wildcard(input)?),
                    '?' | '*' | '|' => {
                        let (_, c) = input.next_unchecked();
                        return Err(ParseError::UnexpectedToken {
                            span: input.current_span(),
                            token: c,
                            expected: Self::EXPR_START_EXPECTED.into(),
                        });
                    }
                    _ => Some(self.parse_single(input)?),
                },
                None => {
                    return Err(ParseError::EmptyExpression {
                        span: input.current_span(),
                    })
                }
            };
        }

        let mut lhs = lhs.unwrap();
        while let Some((_, c)) = input.peek() {
            lhs = match c {
                ')' if parenthesized => break,
                '*' => {
                    if self.postfix_bp(&PostfixOp::Star).0 < min_bp {
                        break;
                    }

                    let _star = input.next_unchecked();
                    self.engine.handle_star(lhs)
                }
                '+' => {
                    if self.postfix_bp(&PostfixOp::Plus).0 < min_bp {
                        break;
                    }

                    let _plus = input.next_unchecked();
                    self.engine.handle_plus(lhs)
                }
                '?' => {
                    if self.postfix_bp(&PostfixOp::Optional).0 < min_bp {
                        break;
                    }

                    let _question = input.next_unchecked();
                    self.engine.handle_optional(lhs)
                }
                '|' => {
                    let (lbp, rbp) = self.infix_bp(&InfixOp::Alternate);
                    if lbp < min_bp {
                        break;
                    }

                    let _bar = input.next_unchecked();
                    let rhs = self.parse_expr(input, rbp, parenthesized)?;
                    self.engine.handle_alternate(lhs, rhs)
                }
                _ => {
                    let (lbp, rbp) = self.infix_bp(&InfixOp::Concat);
                    if lbp < min_bp {
                        break;
                    }

                    let rhs = self.parse_expr(input, rbp, parenthesized)?;
                    self.engine.handle_concat(lhs, rhs)
                }
            }
        }

        Ok(lhs)
    }

    #[inline]
    fn postfix_bp(&self, op: &PostfixOp) -> (u8, ()) {
        match op {
            PostfixOp::Star => (9, ()),
            PostfixOp::Plus => (9, ()),
            PostfixOp::Optional => (9, ()),
        }
    }

    #[inline]
    fn infix_bp(&self, op: &InfixOp) -> (u8, u8) {
        match op {
            InfixOp::Concat => (7, 8),
            InfixOp::Alternate => (5, 6),
        }
    }

    #[inline]
    fn parse_single_char<'r>(&mut self, input: &mut ParseInput<'r>) -> ParseResult<'r, char> {
        // TODO: Expect any
        let (_, c) = input.next_unwrap(Vec::new)?;
        Ok(c)
    }

    #[inline]
    fn parse_single<'r>(&mut self, input: &mut ParseInput<'r>) -> ParseResult<'r, E::Output> {
        let c = self.parse_single_char(input)?;
        Ok(self.engine.handle_char(c))
    }

    #[inline]
    fn parse_escaped_char<'r>(&mut self, input: &mut ParseInput<'r>) -> ParseResult<'r, char> {
        let _bs = input.next_checked('\\', || vec!['\\']);
        // TODO: How to represent expected any character?
        let (_, c) = input.next_unwrap(Vec::new)?;
        Ok(c)
    }

    #[inline]
    fn parse_escaped_class<'r>(
        &mut self,
        input: &mut ParseInput<'r>,
    ) -> ParseResult<'r, CharClass> {
        let c = self.parse_escaped_char(input)?;
        let c = match c {
            'd' => CharClass::decimal_number(),
            'D' => CharClass::decimal_number().complement(),
            's' => CharClass::whitespace(),
            'S' => CharClass::whitespace().complement(),
            'w' => CharClass::word(),
            'W' => CharClass::word().complement(),
            'n' => CharClass::newline(),
            c => c.into(),
        };
        Ok(c)
    }

    #[inline]
    fn parse_escaped<'r>(&mut self, input: &mut ParseInput<'r>) -> ParseResult<'r, E::Output> {
        let c = self.parse_escaped_class(input)?;
        Ok(self.engine.handle_char(c))
    }

    #[allow(dead_code)]
    #[inline]
    fn parse_single_or_escaped_char<'r>(
        &mut self,
        input: &mut ParseInput<'r>,
    ) -> ParseResult<'r, char> {
        match input.peek() {
            Some((_, '\\')) => self.parse_escaped_char(input),
            Some((_, _)) => self.parse_single_char(input),
            None => Err(ParseError::UnexpectedEof {
                span: input.current_eof_span(),
                expected: vec!['\\'],
            }),
        }
    }

    #[inline]
    fn parse_single_or_escaped_class<'r>(
        &mut self,
        input: &mut ParseInput<'r>,
    ) -> ParseResult<'r, CharClass> {
        let c = match input.peek() {
            Some((_, '\\')) => self.parse_escaped_class(input)?,
            Some((_, _)) => self.parse_single_char(input)?.into(),
            None => {
                return Err(ParseError::UnexpectedEof {
                    span: input.current_eof_span(),
                    expected: vec!['\\'],
                })
            }
        };
        Ok(c)
    }

    #[allow(dead_code)]
    #[inline]
    fn parse_single_or_escaped<'r>(
        &mut self,
        input: &mut ParseInput<'r>,
    ) -> ParseResult<'r, E::Output> {
        match input.peek() {
            Some((_, '\\')) => self.parse_escaped(input),
            Some((_, _)) => self.parse_single(input),
            None => Err(ParseError::UnexpectedEof {
                span: input.current_eof_span(),
                expected: vec!['\\'],
            }),
        }
    }

    #[inline]
    fn parse_group<'r>(
        &mut self,
        input: &mut ParseInput<'r>,
    ) -> ParseResult<'r, Option<E::Output>> {
        let _lp = input.next_checked('(', || vec!['('])?;

        let expr = if !input.peek_is(')') {
            let expr = self.parse_expr(input, 0, true)?;
            Some(expr)
        } else {
            None
        };

        let _rp = input.next_checked(')', || vec![')'])?;

        Ok(expr)
    }

    #[inline]
    fn parse_class<'r>(
        &mut self,
        input: &mut ParseInput<'r>,
    ) -> ParseResult<'r, Option<E::Output>> {
        let _lb = input.next_checked('[', || vec!['['])?;

        let negate = match input.peek() {
            Some((_, '^')) => {
                let _caret = input.next_unchecked();
                true
            }
            Some((_, _)) => false,
            None => {
                return Err(ParseError::UnexpectedEof {
                    span: input.current_eof_span(),
                    // TODO: Expect any
                    expected: vec![']', '^'],
                });
            }
        };

        let mut class = CharClass::new();
        while let Some((_, c)) = input.peek() {
            let start = match c {
                // LB indicates end of char class.
                ']' => break,
                _ => self.parse_single_or_escaped_class(input)?,
            };

            // If a class is found, add it and start over.
            // Otherwise, it's the start of a character range.
            if !start.is_single() {
                class.add_other(start);
                continue;
            }

            match input.peek() {
                Some((_, '-')) => {
                    let _dash = input.next_unchecked();
                    let end = self.parse_single_or_escaped_class(input)?;

                    if !end.is_single() {
                        // start is a single char, end is a class; add both individually, and dash.
                        class.add_other(start);
                        class.add_range(('-', '-').into());
                        class.add_other(end);
                    } else {
                        // start and end are both single chars; create a range.
                        let s = start.ranges.into_iter().last().unwrap().start;
                        let e = end.ranges.into_iter().last().unwrap().start;
                        class.add_range((s, e).into());
                    }
                }
                Some((_, _)) => {
                    let s = start.ranges.into_iter().last().unwrap().start;
                    class.add_range((s, s).into());
                }
                None => {
                    return Err(ParseError::UnexpectedEof {
                        span: input.current_eof_span(),
                        // TODO expect any char
                        expected: vec![']', '-'],
                    });
                }
            };
        }

        let _rb = input.next_checked(']', || vec![']']);
        let v = if !class.is_empty() {
            let class = if negate { class.complement() } else { class };
            Some(self.engine.handle_char(class))
        } else {
            None
        };

        Ok(v)
    }

    #[inline]
    fn parse_wildcard_char<'r>(&mut self, input: &mut ParseInput<'r>) -> ParseResult<'r, char> {
        let (_, c) = input.next_checked('.', || vec!['.'])?;
        Ok(c)
    }

    #[inline]
    fn parse_wildcard<'r>(&mut self, input: &mut ParseInput<'r>) -> ParseResult<'r, E::Output> {
        let _ = self.parse_wildcard_char(input)?;
        Ok(self.engine.handle_wildcard())
    }
}

enum PostfixOp {
    Star,
    Plus,
    Optional,
}

enum InfixOp {
    Alternate,
    Concat,
}

struct ParseInput<'r> {
    expr: &'r str,
    input: Peekable<CharIndices<'r>>,

    next_pos: usize,
    char_pos: usize,
}

impl<'r> ParseInput<'r> {
    #[inline]
    pub fn new(expr: &'r str) -> Self {
        Self {
            expr,
            input: expr.char_indices().peekable(),
            next_pos: 0,
            char_pos: 0,
        }
    }

    #[inline]
    pub fn next(&mut self) -> Option<(usize, char)> {
        let next = self.input.next();
        if let Some((char_pos, _)) = next {
            self.next_pos += 1;
            self.char_pos = char_pos;
        }

        next
    }

    #[inline]
    pub fn next_unwrap<F>(&mut self, expected: F) -> ParseResult<'r, (usize, char)>
    where
        F: Fn() -> Vec<char>,
    {
        match self.next() {
            Some(c) => Ok(c),
            None => Err(ParseError::UnexpectedEof {
                span: self.current_eof_span(),
                expected: expected(),
            }),
        }
    }

    #[inline]
    pub fn next_unchecked(&mut self) -> (usize, char) {
        self.next().unwrap()
    }

    #[inline]
    pub fn next_checked<F>(&mut self, check: char, expected: F) -> ParseResult<'r, (usize, char)>
    where
        F: Fn() -> Vec<char>,
    {
        match self.next() {
            Some(next) if next.1 == check => Ok(next),
            Some(next) => Err(ParseError::UnexpectedToken {
                span: self.current_span(),
                token: next.1,
                expected: expected(),
            }),
            None => Err(ParseError::UnexpectedEof {
                span: self.current_eof_span(),
                expected: expected(),
            }),
        }
    }

    #[inline]
    pub fn peek(&mut self) -> Option<&(usize, char)> {
        self.input.peek()
    }

    #[inline]
    pub fn peek_is(&mut self, expected: char) -> bool {
        match self.peek() {
            Some(peeked) => peeked.1 == expected,
            None => false,
        }
    }

    #[allow(dead_code)]
    #[inline]
    pub fn is_empty(&mut self) -> bool {
        self.input.peek().is_none()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn expr(&self) -> &str {
        self.expr
    }

    #[inline]
    fn current_span(&mut self) -> Span<'r> {
        let pos = if self.next_pos == 0 {
            0
        } else {
            self.next_pos - 1
        };

        let text = match self.input.peek() {
            Some((end, _)) => &self.expr[self.char_pos..*end],
            None => &self.expr[self.char_pos..],
        };

        Span::new(pos, pos, text)
    }

    #[inline]
    fn current_eof_span(&self) -> Span<'r> {
        let pos = self.next_pos;
        Span::new(pos, pos, "")
    }
}

/// Error returned when attempting to parse an invalid regular expression.
#[derive(Debug, thiserror::Error)]
pub enum ParseError<'r> {
    #[error("empty regular expression")]
    EmptyExpression { span: Span<'r> },

    #[error("unexpected token")]
    UnexpectedToken {
        span: Span<'r>,
        token: char,
        expected: Vec<char>,
    },
    #[error("unexpected end-of-file")]
    UnexpectedEof { span: Span<'r>, expected: Vec<char> },

    /// There are an invalid number of operators, or operands are missing.
    #[error("unbalanced operators")]
    UnbalancedOperators { span: Span<'r> },
    /// There are one or more sets of unclosed parentheses.
    #[error("unbalanced operators")]
    UnbalancedParentheses { span: Span<'r> },
    /// Bracketed character classes may not empty.
    #[error("empty character class")]
    EmptyCharacterClass { span: Span<'r> },
}

#[derive(Debug)]
pub struct Span<'r> {
    start: usize,
    end: usize,

    text: &'r str,
}

impl<'r> Span<'r> {
    #[inline]
    pub fn new(start: usize, end: usize, text: &'r str) -> Self {
        Self { start, end, text }
    }

    #[inline]
    pub fn start(&self) -> usize {
        self.start
    }

    #[inline]
    pub fn end(&self) -> usize {
        self.end
    }

    #[inline]
    pub fn text(&self) -> &str {
        self.text
    }
}

pub mod nfa {
    use super::{Parser, ParserEngine};
    use crate::class::CharClass;

    use std::hash::Hash;
    use std::marker::PhantomData;

    use automata::nfa::Transition;
    use automata::NFA;

    pub type NFAParser<T> = Parser<NFAParserEngine<T>>;

    /// A regular expression parser that produces an NFA that describes the same language as the
    /// regular expression. The transitions of the NFA must be derivable from CharClass.
    pub struct NFAParserEngine<T>
    where
        T: Clone + Eq + Hash,
        Transition<T>: From<CharClass>,
    {
        _phantom: PhantomData<T>,
    }

    impl<T> NFAParserEngine<T>
    where
        T: Clone + Eq + Hash,
        Transition<T>: From<CharClass>,
    {
        /// Create a new NFAParser.
        #[inline]
        #[allow(clippy::new_without_default)]
        pub fn new() -> Self {
            NFAParserEngine {
                _phantom: PhantomData,
            }
        }
    }

    impl<T> ParserEngine for NFAParserEngine<T>
    where
        T: Clone + Eq + Hash,
        Transition<T>: From<CharClass>,
    {
        type Output = NFA<T>;

        #[inline]
        fn new() -> Self {
            Self::new()
        }

        #[inline]
        fn handle_char<C>(&mut self, c: C) -> Self::Output
        where
            C: Into<CharClass>,
        {
            let class: CharClass = c.into();
            let transition = class.into();

            let mut nfa = NFA::new();
            let f = nfa.add_state(true);
            nfa.add_transition(nfa.start_state, f, transition);
            nfa
        }

        #[inline]
        fn handle_wildcard(&mut self) -> Self::Output {
            let class = CharClass::all_but_newline();
            self.handle_char(class)
        }

        #[inline]
        fn handle_star(&mut self, lhs: Self::Output) -> Self::Output {
            NFA::kleene_star(&lhs)
        }

        #[inline]
        fn handle_plus(&mut self, lhs: Self::Output) -> Self::Output {
            NFA::concatenation(&NFA::kleene_star(&lhs), &lhs)
        }

        #[inline]
        fn handle_optional(&mut self, lhs: Self::Output) -> Self::Output {
            let c1 = NFA::new_epsilon();
            NFA::union(&c1, &lhs)
        }

        #[inline]
        fn handle_concat(&mut self, lhs: Self::Output, rhs: Self::Output) -> Self::Output {
            NFA::concatenation(&lhs, &rhs)
        }

        #[inline]
        fn handle_alternate(&mut self, lhs: Self::Output, rhs: Self::Output) -> Self::Output {
            NFA::union(&lhs, &rhs)
        }
    }
}

pub mod ast {
    use super::{Parser, ParserEngine};
    use crate::ast;
    use crate::class::CharClass;

    use std::hash::Hash;
    use std::marker::PhantomData;

    pub type ASTParser<T> = Parser<ASTParserEngine<T>>;

    /// A regular expression parser that produces an AST that describes the same language as the
    /// regular expression. The transitions of the AST must be derivable from CharClass.
    pub struct ASTParserEngine<T>
    where
        T: Clone + Eq + Hash,
    {
        _phantom: PhantomData<T>,
    }

    impl<T> ASTParserEngine<T>
    where
        T: Clone + Eq + Hash,
    {
        /// Create a new ASTParser.
        #[inline]
        #[allow(clippy::new_without_default)]
        pub fn new() -> Self {
            ASTParserEngine {
                _phantom: PhantomData,
            }
        }
    }

    impl<T> ParserEngine for ASTParserEngine<T>
    where
        T: Clone + Eq + Hash,
    {
        type Output = ast::Expr;

        #[inline]
        fn new() -> Self {
            Self::new()
        }

        #[inline]
        fn handle_char<C>(&mut self, c: C) -> Self::Output
        where
            C: Into<CharClass>,
        {
            let class: CharClass = c.into();
            ast::Expr::Atom(class)
        }

        #[inline]
        fn handle_wildcard(&mut self) -> Self::Output {
            let class = CharClass::all_but_newline();
            self.handle_char(class)
        }

        #[inline]
        fn handle_star(&mut self, lhs: Self::Output) -> Self::Output {
            ast::Expr::Unary(ast::UnaryOp::Star, Box::new(lhs))
        }

        #[inline]
        fn handle_plus(&mut self, rhs: Self::Output) -> Self::Output {
            let lhs = self.handle_star(rhs.clone());
            self.handle_concat(lhs, rhs)
        }

        #[inline]
        fn handle_optional(&mut self, lhs: Self::Output) -> Self::Output {
            ast::Expr::Unary(ast::UnaryOp::Optional, Box::new(lhs))
        }

        #[inline]
        fn handle_concat(&mut self, lhs: Self::Output, rhs: Self::Output) -> Self::Output {
            ast::Expr::Binary(ast::BinaryOp::Concat, Box::new(lhs), Box::new(rhs))
        }

        #[inline]
        fn handle_alternate(&mut self, lhs: Self::Output, rhs: Self::Output) -> Self::Output {
            ast::Expr::Binary(ast::BinaryOp::Alternate, Box::new(lhs), Box::new(rhs))
        }
    }
}
