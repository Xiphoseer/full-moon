pub mod owned;
#[macro_use]
mod parser_util;
mod parsers;
pub mod punctuated;
pub mod span;

use crate::{
    tokenizer::{Symbol, Token, TokenKind, TokenReference, TokenType},
    util::*,
};
use derive_more::Display;
use full_moon_derive::{Node, Owned, Visit};
use generational_arena::Arena;
use itertools::Itertools;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, fmt, iter::FromIterator, sync::Arc};

use parser_util::{
    InternalAstError, OneOrMore, Parser, ParserState, ZeroOrMore, ZeroOrMoreDelimited,
};

use punctuated::{Pair, Punctuated};
use span::ContainedSpan;

#[cfg(feature = "roblox")]
pub mod types;
#[cfg(feature = "roblox")]
use types::*;

/// A block of statements, such as in if/do/etc block
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(
    fmt = "{}{}",
    "display_optional_punctuated_vec(stmts)",
    "display_option(&last_stmt.as_ref().map(display_optional_punctuated))"
)]
pub struct Block<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    stmts: Vec<(Stmt<'a>, Option<Cow<'a, TokenReference<'a>>>)>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    last_stmt: Option<(LastStmt<'a>, Option<Cow<'a, TokenReference<'a>>>)>,
}

impl<'a> Block<'a> {
    /// An iterator over the [statements](enum.Stmt.html) in the block, such as `local foo = 1`
    pub fn iter_stmts(&self) -> impl Iterator<Item = &Stmt<'a>> {
        self.stmts.iter().map(|(stmt, _)| stmt)
    }

    #[deprecated(since = "0.5.0", note = "Use last_stmt instead")]
    pub fn last_stmts(&self) -> Option<&LastStmt<'a>> {
        self.last_stmt()
    }

    /// The last statement of the block if one exists, such as `return foo`
    pub fn last_stmt(&self) -> Option<&LastStmt<'a>> {
        Some(&self.last_stmt.as_ref()?.0)
    }
}

/// The last statement of a [`Block`](struct.Block.html)
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum LastStmt<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    /// A `break` statement
    Break(Cow<'a, TokenReference<'a>>),
    /// A `return` statement
    Return(Return<'a>),
}

/// A `return` statement
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}", token, returns)]
pub struct Return<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    token: Cow<'a, TokenReference<'a>>,
    returns: Punctuated<'a, Expression<'a>>,
}

impl<'a> Return<'a> {
    /// The `return` token
    pub fn token(&self) -> &TokenReference<'a> {
        &self.token
    }

    /// The values being returned
    pub fn returns(&self) -> &Punctuated<'a, Expression<'a>> {
        &self.returns
    }
}

/// Fields of a [`TableConstructor`](struct.TableConstructor.html)
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum Field<'a> {
    /// A key in the format of `[expression] = value`
    #[display(
        fmt = "{}{}{}{}{}",
        "brackets.tokens().0",
        "key",
        "brackets.tokens().1",
        "equal",
        "value"
    )]
    ExpressionKey {
        /// The `[...]` part of `[expression] = value`
        #[cfg_attr(feature = "serde", serde(borrow))]
        brackets: ContainedSpan<'a>,
        /// The `expression` part of `[expression] = value`
        key: Expression<'a>,
        /// The `=` part of `[expression] = value`
        equal: Cow<'a, TokenReference<'a>>,
        /// The `value` part of `[expression] = value`
        value: Expression<'a>,
    },

    /// A key in the format of `name = value`
    #[display(fmt = "{}{}{}", "key", "equal", "value")]
    NameKey {
        #[cfg_attr(feature = "serde", serde(borrow))]
        /// The `name` part of `name = value`
        key: Cow<'a, TokenReference<'a>>,
        /// The `=` part of `name = value`
        equal: Cow<'a, TokenReference<'a>>,
        /// The `value` part of `name = value`
        value: Expression<'a>,
    },

    /// A field with no key, just a value (such as `"a"` in `{ "a" }`)
    #[cfg_attr(feature = "serde", serde(borrow))]
    #[display(fmt = "{}", "_0")]
    NoKey(Expression<'a>),
}

/// A [`Field`](enum.Field.html) used when creating a table
/// Second parameter is the separator used (`,` or `;`) if one exists
pub type TableConstructorField<'a> = (Field<'a>, Option<Cow<'a, TokenReference<'a>>>);

/// A table being constructed, such as `{ 1, 2, 3 }` or `{ a = 1 }`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(
    fmt = "{}{}{}",
    "braces.tokens().0",
    "display_optional_punctuated_vec(fields)",
    "braces.tokens().1"
)]
pub struct TableConstructor<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    #[node(full_range)]
    braces: ContainedSpan<'a>,
    fields: Vec<TableConstructorField<'a>>,
}

impl<'a> TableConstructor<'a> {
    /// The braces of the constructor
    pub fn braces(&self) -> &ContainedSpan<'a> {
        &self.braces
    }

    /// An iterator over the [fields](type.TableConstructorField.html) used to create the table
    pub fn iter_fields(&self) -> impl Iterator<Item = &TableConstructorField<'a>> {
        self.fields.iter()
    }
}

/// A binary operation, such as (`+ 3`)
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}", bin_op, rhs)]
#[visit(visit_as = "bin_op")]
pub struct BinOpRhs<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    bin_op: BinOp<'a>,
    rhs: Box<Expression<'a>>,
}

impl<'a> BinOpRhs<'a> {
    /// The binary operation used, the `+` part of `+ 3`
    pub fn bin_op(&self) -> &BinOp<'a> {
        &self.bin_op
    }

    /// The right hand side of the binary operation, the `3` part of `+ 3`
    pub fn rhs(&self) -> &Expression<'a> {
        self.rhs.as_ref()
    }
}

/// An expression, mostly useful for getting values
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum Expression<'a> {
    /// A statement in parentheses, such as `(#list)`
    #[display(
        fmt = "{}{}{}",
        "contained.tokens().0",
        "expression",
        "contained.tokens().1"
    )]
    Parentheses {
        /// The parentheses of the `ParenExpression`
        #[cfg_attr(feature = "serde", serde(borrow))]
        #[node(full_range)]
        contained: ContainedSpan<'a>,
        /// The expression inside the parentheses
        expression: Box<Expression<'a>>,
    },

    /// A unary operation, such as `#list`
    #[display(fmt = "{}{}", "unop", "expression")]
    UnaryOperator {
        /// The unary operation, the `#` part of `#list`
        #[cfg_attr(feature = "serde", serde(borrow))]
        unop: UnOp<'a>,
        /// The expression the operation is being done on, the `list` part of `#list`
        expression: Box<Expression<'a>>,
    },

    /// A value, such as "strings"
    #[cfg_attr(
        not(feature = "roblox"),
        display(fmt = "{}{}", value, "display_option(binop)")
    )]
    #[cfg_attr(
        feature = "roblox",
        display(
            fmt = "{}{}{}",
            value,
            "display_option(binop)",
            "display_option(as_assertion)"
        )
    )]
    Value {
        /// The value itself
        #[cfg_attr(feature = "serde", serde(borrow))]
        value: Box<Value<'a>>,
        /// The binary operation being done, if one exists (the `+ 3` part of `2 + 3`)
        binop: Option<BinOpRhs<'a>>,
        /// What the value is being asserted as using `as`.
        /// Only available when the "roblox" feature flag is enabled.
        #[cfg(feature = "roblox")]
        #[cfg_attr(feature = "serde", serde(borrow))]
        #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
        as_assertion: Option<AsAssertion<'a>>,
    },
}

/// Values that cannot be used standalone, but as part of things such as [statements](enum.Stmt.html)
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum Value<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    /// An anonymous function, such as `function() end)`
    #[display(fmt = "{}{}", "_0.0", "_0.1")]
    Function((Cow<'a, TokenReference<'a>>, FunctionBody<'a>)),
    /// A call of a function, such as `call()`
    #[display(fmt = "{}", "_0")]
    FunctionCall(FunctionCall<'a>),
    /// A table constructor, such as `{ 1, 2, 3 }`
    #[display(fmt = "{}", "_0")]
    TableConstructor(TableConstructor<'a>),
    /// A number token, such as `3.3`
    #[display(fmt = "{}", "_0")]
    Number(Cow<'a, TokenReference<'a>>),
    /// An expression between parentheses, such as `(3 + 2)`
    #[display(fmt = "{}", "_0")]
    ParseExpression(Expression<'a>),
    /// A string token, such as `"hello"`
    #[display(fmt = "{}", "_0")]
    String(Cow<'a, TokenReference<'a>>),
    /// A symbol, such as `true`
    #[display(fmt = "{}", "_0")]
    Symbol(Cow<'a, TokenReference<'a>>),
    /// A more complex value, such as `call().x`
    #[display(fmt = "{}", "_0")]
    Var(Var<'a>),
}

/// A statement that stands alone
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum Stmt<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    /// An assignment, such as `x = 1`
    #[display(fmt = "{}", _0)]
    Assignment(Assignment<'a>),
    /// A do block, `do end`
    #[display(fmt = "{}", _0)]
    Do(Do<'a>),
    /// A function call on its own, such as `call()`
    #[display(fmt = "{}", _0)]
    FunctionCall(FunctionCall<'a>),
    /// A function declaration, such as `function x() end`
    #[display(fmt = "{}", _0)]
    FunctionDeclaration(FunctionDeclaration<'a>),
    /// A generic for loop, such as `for index, value in pairs(list) do end`
    #[display(fmt = "{}", _0)]
    GenericFor(GenericFor<'a>),
    /// An if statement
    #[display(fmt = "{}", _0)]
    If(If<'a>),
    /// A local assignment, such as `local x = 1`
    #[display(fmt = "{}", _0)]
    LocalAssignment(LocalAssignment<'a>),
    /// A local function declaration, such as `local function x() end`
    #[display(fmt = "{}", _0)]
    LocalFunction(LocalFunction<'a>),
    /// A numeric for loop, such as `for index = 1, 10 do end`
    #[display(fmt = "{}", _0)]
    NumericFor(NumericFor<'a>),
    /// A repeat loop
    #[display(fmt = "{}", _0)]
    Repeat(Repeat<'a>),
    /// A while loop
    #[display(fmt = "{}", _0)]
    While(While<'a>),
    /// A type declaration, such as `type Meters = number`
    /// Only available when the "roblox" feature flag is enabled.
    #[cfg(feature = "roblox")]
    TypeDeclaration(TypeDeclaration<'a>),
}

/// A node used before another in cases such as function calling
/// The `("foo")` part of `("foo"):upper()`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum Prefix<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    #[display(fmt = "{}", _0)]
    /// A complicated expression, such as `("foo")`
    Expression(Expression<'a>),
    #[display(fmt = "{}", _0)]
    /// Just a name, such as `foo`
    Name(Cow<'a, TokenReference<'a>>),
}

/// The indexing of something, such as `x.y` or `x["y"]`
/// Values of variants are the keys, such as `"y"`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum Index<'a> {
    /// Indexing in the form of `x["y"]`
    #[display(
        fmt = "{}{}{}",
        "brackets.tokens().0",
        "expression",
        "brackets.tokens().1"
    )]
    Brackets {
        #[cfg_attr(feature = "serde", serde(borrow))]
        /// The `[...]` part of `["y"]`
        brackets: ContainedSpan<'a>,
        /// The `"y"` part of `["y"]`
        expression: Expression<'a>,
    },

    /// Indexing in the form of `x.y`
    #[display(fmt = "{}{}", "dot", "name")]
    Dot {
        #[cfg_attr(feature = "serde", serde(borrow))]
        /// The `.` part of `.y`
        dot: Cow<'a, TokenReference<'a>>,
        /// The `y` part of `.y`
        name: Cow<'a, TokenReference<'a>>,
    },
}

/// Arguments used for a function
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum FunctionArgs<'a> {
    /// Used when a function is called in the form of `call(1, 2, 3)`
    #[display(
        fmt = "{}{}{}",
        "parentheses.tokens().0",
        "arguments",
        "parentheses.tokens().1"
    )]
    Parentheses {
        /// The `1, 2, 3` part of `1, 2, 3`
        #[cfg_attr(feature = "serde", serde(borrow))]
        arguments: Punctuated<'a, Expression<'a>>,
        /// The `(...) part of (1, 2, 3)`
        #[node(full_range)]
        parentheses: ContainedSpan<'a>,
    },
    /// Used when a function is called in the form of `call "foobar"`
    #[cfg_attr(feature = "serde", serde(borrow))]
    #[display(fmt = "{}", "_0")]
    String(Cow<'a, TokenReference<'a>>),
    /// Used when a function is called in the form of `call { 1, 2, 3 }`
    #[display(fmt = "{}", "_0")]
    TableConstructor(TableConstructor<'a>),
}

/// A numeric for loop, such as `for index = 1, 10 do end`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(
    fmt = "{}{}{}{}{}{}{}{}{}{}{}",
    "for_token",
    "index_variable",
    "equal_token",
    "start",
    "start_end_comma",
    "end",
    "display_option(end_step_comma)",
    "display_option(step)",
    "do_token",
    "block",
    "end_token"
)]
pub struct NumericFor<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    for_token: Cow<'a, TokenReference<'a>>,
    index_variable: Cow<'a, TokenReference<'a>>,
    equal_token: Cow<'a, TokenReference<'a>>,
    start: Expression<'a>,
    start_end_comma: Cow<'a, TokenReference<'a>>,
    end: Expression<'a>,
    end_step_comma: Option<Cow<'a, TokenReference<'a>>>,
    step: Option<Expression<'a>>,
    do_token: Cow<'a, TokenReference<'a>>,
    block: Block<'a>,
    end_token: Cow<'a, TokenReference<'a>>,
}

impl<'a> NumericFor<'a> {
    /// The `for` token
    pub fn for_token(&self) -> &TokenReference<'a> {
        &self.for_token
    }

    /// The index identity, `index` in the initial example
    pub fn index_variable(&self) -> &TokenReference<'a> {
        &self.index_variable
    }

    /// The `=` token
    pub fn equal_token(&self) -> &TokenReference<'a> {
        &self.equal_token
    }

    /// The starting point, `1` in the initial example
    pub fn start(&self) -> &Expression<'a> {
        &self.start
    }

    /// The comma in between the starting point and end point
    /// for _ = 1, 10 do
    ///          ^
    pub fn start_end_comma(&self) -> &TokenReference<'a> {
        &self.start_end_comma
    }

    /// The ending point, `10` in the initial example
    pub fn end(&self) -> &Expression<'a> {
        &self.end
    }

    /// The comma in between the ending point and limit, if one exists
    /// for _ = 0, 10, 2 do
    ///              ^
    pub fn end_step_comma(&self) -> Option<&TokenReference<'a>> {
        self.end_step_comma.as_deref()
    }

    /// The step if one exists, `2` in `for index = 0, 10, 2 do end`
    pub fn step(&self) -> Option<&Expression<'a>> {
        self.step.as_ref()
    }

    /// The `do` token
    pub fn do_token(&self) -> &TokenReference<'a> {
        &self.do_token
    }

    /// The code inside the for loop
    pub fn block(&self) -> &Block<'a> {
        &self.block
    }

    /// The `end` token
    pub fn end_token(&self) -> &TokenReference<'a> {
        &self.end_token
    }
}

/// A generic for loop, such as `for index, value in pairs(list) do end`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(
    fmt = "{}{}{}{}{}{}{}",
    "for_token",
    "names",
    "in_token",
    "expr_list",
    "do_token",
    "block",
    "end_token"
)]
pub struct GenericFor<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    for_token: Cow<'a, TokenReference<'a>>,
    names: Punctuated<'a, Cow<'a, TokenReference<'a>>>,
    in_token: Cow<'a, TokenReference<'a>>,
    expr_list: Punctuated<'a, Expression<'a>>,
    do_token: Cow<'a, TokenReference<'a>>,
    block: Block<'a>,
    end_token: Cow<'a, TokenReference<'a>>,
}

impl<'a> GenericFor<'a> {
    /// The `for` token
    pub fn for_token(&self) -> &TokenReference<'a> {
        &self.for_token
    }

    /// Returns the [`Punctuated`](punctuated/struct.Punctuated.html) sequence of names
    /// In `for index, value in pairs(list) do`, iterates over `index` and `value`
    pub fn names(&self) -> &Punctuated<'a, Cow<'a, TokenReference<'a>>> {
        &self.names
    }

    /// The `in` token
    pub fn in_token(&self) -> &TokenReference<'a> {
        &self.in_token
    }

    /// Returns the [`Punctuated`](punctuated/struct.Punctuated.html) sequence of the expressions looped over
    /// In `for index, value in pairs(list) do`, iterates over `pairs(list)`
    pub fn expr_list(&self) -> &Punctuated<'a, Expression<'a>> {
        &self.expr_list
    }

    /// The `do` token
    pub fn do_token(&self) -> &TokenReference<'a> {
        &self.do_token
    }

    /// The code inside the for loop
    pub fn block(&self) -> &Block<'a> {
        &self.block
    }

    /// The `end` token
    pub fn end_token(&self) -> &TokenReference<'a> {
        &self.end_token
    }
}

/// An if statement
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(
    fmt = "{}{}{}{}{}{}{}{}",
    "if_token",
    "condition",
    "then_token",
    "block",
    "display_option(else_if.as_ref().map(join_vec))",
    "display_option(else_token)",
    "display_option(r#else)",
    "end_token"
)]
pub struct If<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    if_token: Cow<'a, TokenReference<'a>>,
    condition: Expression<'a>,
    then_token: Cow<'a, TokenReference<'a>>,
    block: Block<'a>,
    else_if: Option<Vec<ElseIf<'a>>>,
    else_token: Option<Cow<'a, TokenReference<'a>>>,
    #[cfg_attr(feature = "serde", serde(rename = "else"))]
    r#else: Option<Block<'a>>,
    end_token: Cow<'a, TokenReference<'a>>,
}

impl<'a> If<'a> {
    /// The `if` token
    pub fn if_token(&self) -> &TokenReference<'a> {
        &self.if_token
    }

    /// The condition of the if statement, `condition` in `if condition then`
    pub fn condition(&self) -> &Expression<'a> {
        &self.condition
    }

    /// The `then` token
    pub fn then_token(&self) -> &TokenReference<'a> {
        &self.then_token
    }

    /// The block inside the initial if statement
    pub fn block(&self) -> &Block<'a> {
        &self.block
    }

    /// The `else` token if one exists
    pub fn else_token(&self) -> Option<&TokenReference<'a>> {
        self.else_token.as_deref()
    }

    /// If there are `elseif` conditions, returns a vector of them
    /// Expression is the condition, block is the code if the condition is true
    // TODO: Make this return an iterator, and remove Option part entirely?
    pub fn else_if(&self) -> Option<&Vec<ElseIf<'a>>> {
        self.else_if.as_ref()
    }

    /// The code inside an `else` block if one exists
    pub fn else_block(&self) -> Option<&Block<'a>> {
        self.r#else.as_ref()
    }

    /// The `end` token
    pub fn end_token(&self) -> &TokenReference<'a> {
        &self.end_token
    }
}

/// An elseif block in a bigger [`If`](struct.If.html) statement
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}{}{}", "else_if_token", "condition", "then_token", "block")]
pub struct ElseIf<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    else_if_token: Cow<'a, TokenReference<'a>>,
    condition: Expression<'a>,
    then_token: Cow<'a, TokenReference<'a>>,
    block: Block<'a>,
}

impl<'a> ElseIf<'a> {
    /// The `elseif` token
    pub fn else_if_token(&self) -> &TokenReference<'a> {
        &self.else_if_token
    }

    /// The condition of the `elseif`, `condition` in `elseif condition then`
    pub fn condition(&self) -> &Expression<'a> {
        &self.condition
    }

    /// The `then` token
    pub fn then_token(&self) -> &TokenReference<'a> {
        &self.then_token
    }

    /// The body of the `elseif`
    pub fn block(&self) -> &Block<'a> {
        &self.block
    }
}

/// A while loop
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(
    fmt = "{}{}{}{}{}",
    "while_token",
    "condition",
    "do_token",
    "block",
    "end_token"
)]
pub struct While<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    while_token: Cow<'a, TokenReference<'a>>,
    condition: Expression<'a>,
    do_token: Cow<'a, TokenReference<'a>>,
    block: Block<'a>,
    end_token: Cow<'a, TokenReference<'a>>,
}

impl<'a> While<'a> {
    /// The `while` token
    pub fn while_token(&self) -> &TokenReference<'a> {
        &self.while_token
    }

    /// The `condition` part of `while condition do`
    pub fn condition(&self) -> &Expression<'a> {
        &self.condition
    }

    /// The `do` token
    pub fn do_token(&self) -> &TokenReference<'a> {
        &self.do_token
    }

    /// The code inside the while loop
    pub fn block(&self) -> &Block<'a> {
        &self.block
    }

    /// The `end` token
    pub fn end_token(&self) -> &TokenReference<'a> {
        &self.end_token
    }
}

/// A repeat loop
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}{}{}", "repeat_token", "block", "until_token", "until")]
pub struct Repeat<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    repeat_token: Cow<'a, TokenReference<'a>>,
    block: Block<'a>,
    until_token: Cow<'a, TokenReference<'a>>,
    until: Expression<'a>,
}

impl<'a> Repeat<'a> {
    /// The `repeat` token
    pub fn repeat_token(&self) -> &TokenReference<'a> {
        &self.repeat_token
    }

    /// The code inside the `repeat` block
    pub fn block(&self) -> &Block<'a> {
        &self.block
    }

    /// The `until` token
    pub fn until_token(&self) -> &TokenReference<'a> {
        &self.until_token
    }

    /// The condition for the `until` part
    pub fn until(&self) -> &Expression<'a> {
        &self.until
    }
}

/// A method call, such as `x:y()`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}{}", "colon_token", "name", "args")]
pub struct MethodCall<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    colon_token: Cow<'a, TokenReference<'a>>,
    name: Cow<'a, TokenReference<'a>>,
    args: FunctionArgs<'a>,
}

impl<'a> MethodCall<'a> {
    /// The `:` in `x:y()`
    pub fn colon_token(&self) -> &TokenReference<'a> {
        &self.colon_token
    }

    /// The arguments of a method call, the `x, y, z` part of `method:call(x, y, z)`
    pub fn args(&self) -> &FunctionArgs<'a> {
        &self.args
    }

    /// The method being called, the `call` part of `method:call()`
    pub fn name(&self) -> &TokenReference<'a> {
        &self.name
    }
}

/// Something being called
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum Call<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    #[display(fmt = "{}", "_0")]
    /// A function being called directly, such as `x(1)`
    AnonymousCall(FunctionArgs<'a>),
    #[display(fmt = "{}", "_0")]
    /// A method call, such as `x:y()`
    MethodCall(MethodCall<'a>),
}

/// A function body, everything except `function x` in `function x(a, b, c) call() end`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(
    not(feature = "roblox"),
    display(
        fmt = "{}{}{}{}{}",
        "parameters_parantheses.tokens().0",
        "parameters",
        "parameters_parantheses.tokens().1",
        "block",
        "end_token"
    )
)]
#[cfg_attr(
    feature = "roblox",
    display(
        fmt = "{}{}{}{}{}{}{}",
        "parameters_parantheses.tokens().0",
        "parameters",
        "parameters_parantheses.tokens().1",
        "type_specifiers",
        "return_type",
        "block",
        "end_token"
    )
)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub struct FunctionBody<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    parameters_parantheses: ContainedSpan<'a>,
    parameters: Punctuated<'a, Parameter<'a>>,

    #[cfg(feature = "roblox")]
    #[cfg_attr(feature = "serde", serde(borrow))]
    type_specifiers: Vec<Option<TypeSpecifier<'a>>>,

    #[cfg(feature = "roblox")]
    #[cfg_attr(feature = "serde", serde(borrow))]
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    return_type: Option<TypeSpecifier<'a>>,

    block: Block<'a>,
    end_token: Cow<'a, TokenReference<'a>>,
}

impl<'a> FunctionBody<'a> {
    /// The parentheses of the parameters
    pub fn parameters_parantheses(&self) -> &ContainedSpan<'a> {
        &self.parameters_parantheses
    }

    /// An iterator over the parameters for the function declaration
    pub fn iter_parameters(&self) -> impl Iterator<Item = &Parameter<'a>> {
        self.parameters.iter()
    }

    /// The code of a function body
    pub fn block(&self) -> &Block<'a> {
        &self.block
    }

    /// The `end` token
    pub fn end_token(&self) -> &TokenReference<'a> {
        &self.end_token
    }

    /// The type specifiers of the variables, in the order that they were assigned.
    /// `(foo: number, bar, baz: boolean)` returns an iterator containing:
    /// `Some(TypeSpecifier(number)), None, Some(TypeSpecifier(boolean))`
    /// Only available when the "roblox" feature flag is enabled.
    #[cfg(feature = "roblox")]
    pub fn type_specifiers(&self) -> impl Iterator<Item = Option<&TypeSpecifier<'a>>> {
        self.type_specifiers.iter().map(Option::as_ref)
    }

    /// The return type of the function, if one exists.
    /// Only available when the "roblox" feature flag is enabled.
    #[cfg(feature = "roblox")]
    pub fn return_type(&self) -> Option<&TypeSpecifier<'a>> {
        self.return_type.as_ref()
    }
}

/// A parameter in a function declaration
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum Parameter<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    /// The `...` vararg syntax, such as `function x(...)`
    Ellipse(Cow<'a, TokenReference<'a>>),
    /// A name parameter, such as `function x(a, b, c)`
    Name(Cow<'a, TokenReference<'a>>),
}

/// A suffix in certain cases, such as `:y()` in `x:y()`
/// Can be stacked on top of each other, such as in `x()()()`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum Suffix<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    #[display(fmt = "{}", "_0")]
    /// A call, including method calls and direct calls
    Call(Call<'a>),
    #[display(fmt = "{}", "_0")]
    /// An index, such as `x.y`
    Index(Index<'a>),
}

/// A complex expression used by [`Var`](enum.Var.html), consisting of both a prefix and suffixes
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}", "prefix", "join_vec(suffixes)")]
pub struct VarExpression<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    prefix: Prefix<'a>,
    suffixes: Vec<Suffix<'a>>,
}

impl<'a> VarExpression<'a> {
    /// The prefix of the expression, such as a name
    pub fn prefix(&self) -> &Prefix<'a> {
        &self.prefix
    }

    /// An iter over the suffixes, such as indexing or calling
    pub fn iter_suffixes(&self) -> impl Iterator<Item = &Suffix<'a>> {
        self.suffixes.iter()
    }
}

/// Used in [`Assignment`s](struct.Assignment.html) and [`Value`s](enum.Value.html)
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum Var<'a> {
    /// An expression, such as `x.y.z` or `x()`
    #[cfg_attr(feature = "serde", serde(borrow))]
    #[display(fmt = "{}", "_0")]
    Expression(VarExpression<'a>),
    /// A literal identifier, such as `x`
    #[display(fmt = "{}", "_0")]
    Name(Cow<'a, TokenReference<'a>>),
}

/// An assignment, such as `x = y`. Not used for [`LocalAssignment`s](struct.LocalAssignment.html)
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}{}", "var_list", "equal_token", "expr_list")]
pub struct Assignment<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    var_list: Punctuated<'a, Var<'a>>,
    equal_token: Cow<'a, TokenReference<'a>>,
    expr_list: Punctuated<'a, Expression<'a>>,
}

impl<'a> Assignment<'a> {
    /// Returns the [`Punctuated`](punctuated/struct.Punctuated.html) sequence over the expressions being assigned.
    /// This is the the `1, 2` part of `x, y["a"] = 1, 2`
    pub fn expr_list(&self) -> &Punctuated<'a, Expression<'a>> {
        &self.expr_list
    }

    /// The `=` token in between `x = y`
    pub fn equal_token(&self) -> &TokenReference<'a> {
        &self.equal_token
    }

    /// Returns the [`Punctuated`](punctuated/struct.Punctuated.html) sequence over the variables being assigned to.
    /// This is the `x, y["a"]` part of `x, y["a"] = 1, 2`
    pub fn var_list(&self) -> &Punctuated<'a, Var<'a>> {
        &self.var_list
    }
}

/// A declaration of a local function, such as `local function x() end`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}{}{}", "local_token", "function_token", "name", "func_body")]
pub struct LocalFunction<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    local_token: Cow<'a, TokenReference<'a>>,
    function_token: Cow<'a, TokenReference<'a>>,
    name: Cow<'a, TokenReference<'a>>,
    func_body: FunctionBody<'a>,
}

impl<'a> LocalFunction<'a> {
    /// The `local` token
    pub fn local_token(&self) -> &TokenReference<'a> {
        &self.local_token
    }

    /// The `function` token
    pub fn function_token(&self) -> &TokenReference<'a> {
        &self.function_token
    }

    /// The function body, everything except `local function x` in `local function x(a, b, c) call() end`
    pub fn func_body(&self) -> &FunctionBody<'a> {
        &self.func_body
    }

    /// The name of the function, the `x` part of `local function x() end`
    pub fn name(&self) -> &TokenReference<'a> {
        &self.name
    }
}

/// An assignment to a local variable, such as `local x = 1`
#[derive(Clone, Debug, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub struct LocalAssignment<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    local_token: Cow<'a, TokenReference<'a>>,
    #[cfg(feature = "roblox")]
    #[cfg_attr(feature = "serde", serde(borrow))]
    type_specifiers: Vec<Option<TypeSpecifier<'a>>>,
    name_list: Punctuated<'a, Cow<'a, TokenReference<'a>>>,
    equal_token: Option<Cow<'a, TokenReference<'a>>>,
    expr_list: Punctuated<'a, Expression<'a>>,
}

impl<'a> LocalAssignment<'a> {
    /// The `local` token
    pub fn local_token(&self) -> &TokenReference<'a> {
        &self.local_token
    }

    /// The `=` token in between `local x = y`, if one exists
    pub fn equal_token(&self) -> Option<&TokenReference<'a>> {
        self.equal_token.as_deref()
    }

    /// Returns the [`Punctuated`](punctuated/struct.Punctuated.html) sequence of the expressions being assigned.
    /// This is the `1, 2` part of `local x, y = 1, 2`
    pub fn expr_list(&self) -> &Punctuated<'a, Expression<'a>> {
        &self.expr_list
    }

    /// Returns the [`Punctuated`](punctuated/struct.Punctuated.html) sequence of names being assigned to.
    /// This is the `x, y` part of `local x, y = 1, 2`
    pub fn name_list(&self) -> &Punctuated<'a, Cow<'a, TokenReference<'a>>> {
        &self.name_list
    }

    /// Returns a mutable [`Punctuated`](punctuated/struct.Punctuated.html) sequence of names being assigned to.
    /// This is the `x, y` part of `local x, y = 1, 2`
    pub fn name_list_mut(&mut self) -> &mut Punctuated<'a, Cow<'a, TokenReference<'a>>> {
        &mut self.name_list
    }

    /// The type specifiers of the variables, in the order that they were assigned.
    /// `local foo: number, bar, baz: boolean` returns an iterator containing:
    /// `Some(TypeSpecifier(number)), None, Some(TypeSpecifier(boolean))`
    /// Only available when the "roblox" feature flag is enabled.
    #[cfg(feature = "roblox")]
    pub fn type_specifiers(&self) -> impl Iterator<Item = Option<&TypeSpecifier<'a>>> {
        self.type_specifiers.iter().map(Option::as_ref)
    }
}

impl fmt::Display for LocalAssignment<'_> {
    #[cfg(feature = "roblox")]
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        unimplemented!("Display impl for LocalAssignment in the Roblox feature flag")
    }

    #[cfg(not(feature = "roblox"))]
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}", self.local_token)?;
        write!(formatter, "{}", self.name_list)?;
        write!(formatter, "{}", display_option(&self.equal_token))?;
        write!(formatter, "{}", self.expr_list)
    }
}

/// A `do` block, such as `do ... end`
/// This is not used for things like `while true do end`, only those on their own
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}{}", "do_token", "block", "end_token")]
pub struct Do<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    do_token: Cow<'a, TokenReference<'a>>,
    block: Block<'a>,
    end_token: Cow<'a, TokenReference<'a>>,
}

impl<'a> Do<'a> {
    /// The `do` token
    pub fn do_token(&self) -> &TokenReference<'a> {
        &self.do_token
    }

    /// The code inside the `do ... end`
    pub fn block(&self) -> &Block<'a> {
        &self.block
    }

    /// The `end` token
    pub fn end_token(&self) -> &TokenReference<'a> {
        &self.end_token
    }
}

/// A function being called, such as `call()`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}", "prefix", "join_vec(suffixes)")]
pub struct FunctionCall<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    prefix: Prefix<'a>,
    suffixes: Vec<Suffix<'a>>,
}

impl<'a> FunctionCall<'a> {
    /// The prefix of a function call, the `call` part of `call()`
    pub fn prefix(&self) -> &Prefix<'a> {
        &self.prefix
    }

    /// The suffix of a function call, the `()` part of `call()`
    pub fn iter_suffixes(&self) -> impl Iterator<Item = &Suffix<'a>> {
        self.suffixes.iter()
    }
}

/// A function name when being [declared](struct.FunctionDeclaration.html)
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(
    fmt = "{}{}{}",
    "names",
    "display_option(self.method_colon())",
    "display_option(self.method_name())"
)]
pub struct FunctionName<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    names: Punctuated<'a, Cow<'a, TokenReference<'a>>>,
    colon_name: Option<(Cow<'a, TokenReference<'a>>, Cow<'a, TokenReference<'a>>)>,
}

impl<'a> FunctionName<'a> {
    /// The colon between the name and the method, the `:` part of `function x:y() end`
    pub fn method_colon(&self) -> Option<&TokenReference<'a>> {
        Some(&self.colon_name.as_ref()?.0)
    }

    /// A method name if one exists, the `y` part of `function x:y() end`
    pub fn method_name(&self) -> Option<&TokenReference<'a>> {
        Some(&self.colon_name.as_ref()?.1)
    }

    /// Returns the [`Punctuated`](punctuated/struct.Punctuated.html) sequence over the names used when defining the function.
    /// This is the `x.y.z` part of `function x.y.z() end`
    pub fn names(&self) -> &Punctuated<'a, Cow<'a, TokenReference<'a>>> {
        &self.names
    }
}

/// A normal function declaration, supports simple declarations like `function x() end`
/// as well as complicated declarations such as `function x.y.z:a() end`
#[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
#[display(fmt = "{}{}{}", "function_token", "name", "body")]
pub struct FunctionDeclaration<'a> {
    #[cfg_attr(feature = "serde", serde(borrow))]
    function_token: Cow<'a, TokenReference<'a>>,
    name: FunctionName<'a>,
    body: FunctionBody<'a>,
}

impl<'a> FunctionDeclaration<'a> {
    /// The `function` token
    pub fn function_token(&self) -> &TokenReference<'a> {
        &self.function_token
    }

    /// The body of the function
    pub fn body(&self) -> &FunctionBody<'a> {
        &self.body
    }

    /// The name of the function
    pub fn name(&self) -> &FunctionName<'a> {
        &self.name
    }
}

macro_rules! make_op {
    ($enum:ident, $(#[$outer:meta])* { $($operator:ident,)+ }) => {
        #[derive(Clone, Debug, Display, PartialEq, Owned, Node, Visit)]
        #[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
        #[visit(skip_visit_self)]
        $(#[$outer])*
        #[display(fmt = "{}")]
        pub enum $enum<'a> {
            #[cfg_attr(feature = "serde", serde(borrow))]
            $(
                #[allow(missing_docs)]
                $operator(Cow<'a, TokenReference<'a>>),
            )+
        }
    };
}

make_op!(BinOp,
    #[doc = "Operators that require two operands, such as X + Y or X - Y"]
    {
        And,
        Caret,
        GreaterThan,
        GreaterThanEqual,
        LessThan,
        LessThanEqual,
        Minus,
        Or,
        Percent,
        Plus,
        Slash,
        Star,
        TildeEqual,
        TwoDots,
        TwoEqual,
    }
);

make_op!(UnOp,
    #[doc = "Operators that require just one operand, such as #X"]
    {
        Minus,
        Not,
        Hash,
    }
);

/// An error that occurs when creating the ast *after* tokenizing
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Deserialize, Serialize))]
pub enum AstError<'a> {
    /// There were no tokens passed, which shouldn't happen normally
    Empty,
    /// Tokens passed had no end of file token, which shouldn't happen normally
    NoEof,
    /// An unexpected token, the most likely scenario when getting an AstError
    UnexpectedToken {
        /// The token that caused the error
        #[cfg_attr(feature = "serde", serde(borrow))]
        token: Token<'a>,
        /// Any additional information that could be provided for debugging
        additional: Option<Cow<'a, str>>,
    },
}

impl<'a> fmt::Display for AstError<'a> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AstError::Empty => write!(formatter, "tokens passed was empty, which shouldn't happen normally"),
            AstError::NoEof => write!(formatter, "tokens passed had no eof token, which shouldn't happen normally"),
            AstError::UnexpectedToken { token, additional } => write!(
                formatter,
                "unexpected token `{}`. (starting from line {}, character {} and ending on line {}, character {}){}",
                token,
                token.start_position().line(),
                token.start_position().character(),
                token.end_position().line(),
                token.end_position().character(),
                match additional {
                    Some(additional) => format!("\nadditional information: {}", additional),
                    None => String::new(),
                }
            )
        }
    }
}

impl<'a> std::error::Error for AstError<'a> {}

/// An abstract syntax tree, contains all the nodes used in the code
#[derive(Clone, Debug, Owned)]
pub struct Ast<'a> {
    nodes: Block<'a>,
    tokens: Vec<TokenReference<'a>>,
}

impl<'a> Ast<'a> {
    /// Create an Ast from the passed tokens. You probably want [`parse`](../fn.parse.html)
    ///
    /// # Errors
    ///
    /// If the tokens passed are impossible to get through normal tokenization,
    /// an error of Empty (if the vector is empty) or NoEof (if there is no eof token)
    /// will be returned.
    ///
    /// More likely, if the tokens pass are invalid Lua 5.1 code, an
    /// UnexpectedToken error will be returned.
    pub fn from_tokens(tokens: Vec<Token<'a>>) -> Result<Ast<'a>, AstError<'a>> {
        if *tokens.last().ok_or(AstError::Empty)?.token_type() != TokenType::Eof {
            Err(AstError::NoEof)
        } else {
            let tokens = extract_token_references(tokens);
            let mut state = ParserState::new(&tokens);

            if tokens
                .iter()
                .filter(|token| !token.token_type().ignore())
                .count()
                == 1
            {
                // Entirely comments/whitespace
                return Ok(Ast {
                    nodes: Block {
                        stmts: Vec::new(),
                        last_stmt: None,
                    },
                    tokens,
                });
            }

            // ParserState has to have at least 2 tokens, the last being an EOF, thus unwrap() can't fail
            if state.peek().token_type().ignore() {
                state = state.advance().unwrap();
            }

            match parsers::ParseBlock.parse(state.clone()) {
                Ok((state, block)) => {
                    if state.index == tokens.len() - 1 {
                        // TODO: Can we avoid clone()?
                        Ok(Ast {
                            nodes: block,
                            tokens,
                        })
                    } else {
                        Err(AstError::UnexpectedToken {
                            token: (*state.peek()).to_owned().token,
                            additional: Some(Cow::Borrowed("leftover token")),
                        })
                    }
                }

                Err(InternalAstError::NoMatch) => Err(AstError::UnexpectedToken {
                    token: (*state.peek()).to_owned().token,
                    additional: None,
                }),

                Err(InternalAstError::UnexpectedToken { token, additional }) => {
                    Err(AstError::UnexpectedToken {
                        token: (*token).to_owned(),
                        additional: additional.map(Cow::Borrowed),
                    })
                }
            }
        }
    }

    /// The entire code of the function
    ///
    /// ```rust
    /// # fn main() -> Result<(), Box<std::error::Error>> {
    /// assert_eq!(full_moon::parse("local x = 1; local y = 2")?.nodes().iter_stmts().count(), 2);
    /// # Ok(())
    /// # }
    /// ```
    pub fn nodes(&self) -> &Block<'a> {
        &self.nodes
    }

    /// The entire code of the function, but mutable
    pub fn nodes_mut(&mut self) -> &mut Block<'a> {
        &mut self.nodes
    }

    /// The EOF token at the end of every Ast
    pub fn eof(&self) -> &TokenReference<'a> {
        self.tokens.last().expect("no eof token, somehow?")
    }

    /// An iterator over the tokens used to create the Ast
    pub fn iter_tokens(&self) -> impl Iterator<Item = &Token<'a>> {
        // self.tokens.iter().map(|(_, token)| token).sorted()
        unimplemented!("Ast::iter_tokens");
        None.iter()
    }

    /// Will update the positions of all the tokens in the tree
    /// Necessary if you are both mutating the tree and need the positions of the tokens
    pub fn update_positions(&mut self) {
        unimplemented!(
            "Ast::update_positions is going to just create a clone of the token, probably"
        );

        // use crate::tokenizer::Position;

        // let mut start_position = Position {
        //     bytes: 0,
        //     character: 1,
        //     line: 1,
        // };

        // let mut next_is_new_line = false;

        // for (_, token) in self.tokens.iter() {
        //     let display = token.to_string();

        //     let mut lines = bytecount::count(&display.as_bytes(), b'\n');
        //     if token.token_kind() == TokenKind::Whitespace {
        //         lines = lines.saturating_sub(1);
        //     }

        //     let end_position = if token.token_kind() == TokenKind::Eof {
        //         start_position
        //     } else {
        //         let mut end_position = Position {
        //             bytes: start_position.bytes() + display.len(),
        //             line: start_position.line() + lines,
        //             character: {
        //                 let offset = display.lines().last().unwrap_or("").chars().count();
        //                 if lines > 0 || next_is_new_line {
        //                     offset + 1
        //                 } else {
        //                     start_position.character() + offset
        //                 }
        //             },
        //         };

        //         if next_is_new_line {
        //             end_position.line += 1;
        //             next_is_new_line = false;
        //         }

        //         end_position
        //     };

        //     if display.ends_with('\n') {
        //         next_is_new_line = true;
        //     }
        //
        // token.start_position.store(start_position);
        // token.end_position.store(end_position);
        // start_position = end_position;
        // }
    }
}

/// Extracts leading and trailing trivia from tokens
pub(crate) fn extract_token_references<'a>(mut tokens: Vec<Token<'a>>) -> Vec<TokenReference<'a>> {
    let mut references = Vec::new();
    let (mut leading_trivia, mut trailing_trivia) = (Vec::new(), Vec::new());
    let mut tokens = tokens.drain(..).peekable();

    while let Some(token) = tokens.next() {
        if token.token_type().is_trivia() {
            leading_trivia.push(token);
        } else {
            while let Some(token) = tokens.peek() {
                if token.token_type().is_trivia() {
                    if let TokenType::Whitespace { ref characters } = &*token.token_type() {
                        if characters.starts_with('\n') {
                            break;
                        }
                    }

                    trailing_trivia.push(tokens.next().unwrap());
                } else {
                    break;
                }
            }

            references.push(TokenReference {
                leading_trivia: leading_trivia.drain(..).collect(),
                trailing_trivia: trailing_trivia.drain(..).collect(),
                token,
            });
        }
    }

    references
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::tokens;

    #[test]
    fn test_extract_token_references() {
        let tokens = tokens("print(1)\n-- hello world\nlocal foo -- this is the word foo").unwrap();

        let references = extract_token_references(tokens);
        assert_eq!(references.len(), 7);

        assert!(references[0].trailing_trivia.is_empty());
        assert_eq!(references[0].token.to_string(), "print");
        assert!(references[0].leading_trivia.is_empty());

        assert!(references[1].trailing_trivia.is_empty());
        assert_eq!(references[1].token.to_string(), "(");
        assert!(references[1].leading_trivia.is_empty());

        assert!(references[2].trailing_trivia.is_empty());
        assert_eq!(references[2].token.to_string(), "1");
        assert!(references[2].leading_trivia.is_empty());

        assert_eq!(references[4].leading_trivia[0].to_string(), "\n");

        assert_eq!(
            references[4].leading_trivia[1].to_string(),
            "-- hello world",
        );

        assert_eq!(references[4].leading_trivia[2].to_string(), "\n");
        assert_eq!(references[4].token.to_string(), "local");
        assert_eq!(references[4].trailing_trivia[0].to_string(), " ");
    }
}
