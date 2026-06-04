// Copyright 2022 TiKV Project Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This crate provides a macro that can be used to append a match expression
//! with multiple arms, where the tokens in the first arm, as a template, can be
//! substituted and the template arm will be expanded into multiple arms.
//!
//! For example, the following code
//!
//! ```ignore
//! match_template! {
//!     T = [Int, Real, Double],
//!     match Foo {
//!         EvalType::T => { panic!("{}", EvalType::T); },
//!         EvalType::Other => unreachable!(),
//!     }
//! }
//! ```
//!
//! generates
//!
//! ```ignore
//! match Foo {
//!     EvalType::Int => { panic!("{}", EvalType::Int); },
//!     EvalType::Real => { panic!("{}", EvalType::Real); },
//!     EvalType::Double => { panic!("{}", EvalType::Double); },
//!     EvalType::Other => unreachable!(),
//! }
//! ```
//!
//! In addition, substitution can vary on two sides of the arms.
//!
//! For example,
//!
//! ```ignore
//! match_template! {
//!     T = [Foo, Bar => Baz],
//!     match Foo {
//!         EvalType::T => { panic!("{}", EvalType::T); },
//!     }
//! }
//! ```
//!
//! generates
//!
//! ```ignore
//! match Foo {
//!     EvalType::Foo => { panic!("{}", EvalType::Foo); },
//!     EvalType::Bar => { panic!("{}", EvalType::Baz); },
//! }
//! ```
//!
//! To reference both sides of a mapped substitution, use a pair of template
//! identifiers.
//!
//! For example,
//!
//! ```ignore
//! match_template! {
//!     (VN, VT) = [Databases => DatabasesView, Schemas => SchemasView],
//!     match table_name {
//!         VT::TABLE_NAME => Some(SystemView::VN(VT)),
//!         _ => None,
//!     }
//! }
//! ```
//!
//! generates
//!
//! ```ignore
//! match table_name {
//!     DatabasesView::TABLE_NAME => Some(SystemView::Databases(DatabasesView)),
//!     SchemasView::TABLE_NAME => Some(SystemView::Schemas(SchemasView)),
//!     _ => None,
//! }
//! ```
//!
//! Wildcard match arm is also supported (but there will be no substitution).

use proc_macro2::Group;
use proc_macro2::Ident;
use proc_macro2::TokenStream;
use proc_macro2::TokenTree;
use quote::quote;
use quote::ToTokens;
use syn::bracketed;
use syn::parenthesized;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse_macro_input;
use syn::punctuated::Punctuated;
use syn::Arm;
use syn::Expr;
use syn::ExprMatch;
use syn::Pat;
use syn::Token;

/// A procedural macro that generates repeated match arms by pattern.
///
/// See the [crate documentation](self) for more details.
#[proc_macro]
pub fn match_template(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mt = parse_macro_input!(input as MatchTemplate);
    mt.expand().into()
}

struct MatchTemplate {
    template: Template,
    substitutes: Punctuated<Substitution, Token![,]>,
    match_exp: Box<Expr>,
    template_arm: Arm,
    remaining_arms: Vec<Arm>,
}

impl Parse for MatchTemplate {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let template = input.parse()?;
        input.parse::<Token![=]>()?;
        let substitutes_tokens;
        bracketed!(substitutes_tokens in input);
        let substitutes =
            Punctuated::<Substitution, Token![,]>::parse_terminated(&substitutes_tokens)?;
        input.parse::<Token![,]>()?;
        let m: ExprMatch = input.parse()?;
        let mut arms = m.arms;
        arms.iter_mut().for_each(|arm| arm.comma = None);
        assert!(!arms.is_empty(), "Expect at least 1 match arm");
        let template_arm = arms.remove(0);
        assert!(template_arm.guard.is_none(), "Expect no match arm guard");

        Ok(Self {
            template,
            substitutes,
            match_exp: m.expr,
            template_arm,
            remaining_arms: arms,
        })
    }
}

impl MatchTemplate {
    fn expand(self) -> TokenStream {
        let Self {
            template,
            substitutes,
            match_exp,
            template_arm,
            remaining_arms,
        } = self;
        let match_arms = substitutes.into_iter().map(|substitute| {
            let mut arm = template_arm.clone();
            let (left_tokens, right_tokens) = match substitute {
                Substitution::Identical(ident) => {
                    (ident.clone().into_token_stream(), ident.into_token_stream())
                }
                Substitution::Map(left_ident, right_tokens) => {
                    (left_ident.into_token_stream(), right_tokens)
                }
            };
            match &template {
                Template::Single(template_ident) => {
                    arm.pat = replace_in_token_stream(
                        arm.pat,
                        Pat::parse_multi_with_leading_vert,
                        template_ident,
                        &left_tokens,
                    );
                    arm.body = replace_in_token_stream(
                        arm.body,
                        Parse::parse,
                        template_ident,
                        &right_tokens,
                    );
                }
                Template::Pair {
                    left_ident,
                    right_ident,
                } => {
                    let replacements = [(left_ident, &left_tokens), (right_ident, &right_tokens)];
                    arm.pat = replace_all_in_token_stream(
                        arm.pat,
                        Pat::parse_multi_with_leading_vert,
                        &replacements,
                    );
                    arm.body = replace_all_in_token_stream(arm.body, Parse::parse, &replacements);
                }
            }
            arm
        });
        quote! {
            match #match_exp {
                #(#match_arms,)*
                #(#remaining_arms,)*
            }
        }
    }
}

#[derive(Debug)]
enum Template {
    Single(Ident),
    Pair {
        left_ident: Ident,
        right_ident: Ident,
    },
}

impl Parse for Template {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.peek(syn::token::Paren) {
            let template_tokens;
            parenthesized!(template_tokens in input);
            let left_ident = template_tokens.parse()?;
            template_tokens.parse::<Token![,]>()?;
            let right_ident = template_tokens.parse()?;
            if !template_tokens.is_empty() {
                return Err(template_tokens.error("expected exactly two template identifiers"));
            }
            Ok(Template::Pair {
                left_ident,
                right_ident,
            })
        } else {
            Ok(Template::Single(input.parse()?))
        }
    }
}

#[derive(Debug)]
enum Substitution {
    Identical(Ident),
    Map(Ident, TokenStream),
}

impl Parse for Substitution {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let left_ident = input.parse()?;
        let fat_arrow: Option<Token![=>]> = input.parse()?;
        if fat_arrow.is_some() {
            let mut right_tokens: Vec<TokenTree> = vec![];
            while !input.peek(Token![,]) && !input.is_empty() {
                right_tokens.push(input.parse()?);
            }
            Ok(Substitution::Map(
                left_ident,
                right_tokens.into_iter().collect(),
            ))
        } else {
            Ok(Substitution::Identical(left_ident))
        }
    }
}

fn replace_in_token_stream<T: ToTokens, P: Fn(ParseStream) -> syn::Result<T>>(
    input: T,
    parse: P,
    from_ident: &Ident,
    to_tokens: &TokenStream,
) -> T {
    replace_all_in_token_stream(input, parse, &[(from_ident, to_tokens)])
}

fn replace_all_in_token_stream<T: ToTokens, P: Fn(ParseStream) -> syn::Result<T>>(
    input: T,
    parse: P,
    replacements: &[(&Ident, &TokenStream)],
) -> T {
    let mut tokens = TokenStream::new();
    input.to_tokens(&mut tokens);

    let tokens = replace_tokens(tokens, replacements);
    syn::parse::Parser::parse2(parse, tokens).unwrap()
}

fn replace_tokens(tokens: TokenStream, replacements: &[(&Ident, &TokenStream)]) -> TokenStream {
    tokens
        .into_iter()
        .flat_map(|token| match token {
            TokenTree::Ident(ident) => replacements
                .iter()
                .find_map(|(from_ident, to_tokens)| {
                    if ident == **from_ident {
                        Some((*to_tokens).clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| ident.into_token_stream()),
            TokenTree::Group(group) => {
                let mut new_group = Group::new(
                    group.delimiter(),
                    replace_tokens(group.stream(), replacements),
                );
                new_group.set_span(group.span());
                new_group.into_token_stream()
            }
            other => other.into(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let input = r#"
            T = [Int, Real, Double],
            match foo() {
                EvalType::T => { panic!("{}", EvalType::T); },
                EvalType::Other => unreachable!(),
            }
        "#;

        let expect_output = r#"
            match foo() {
                EvalType::Int => { panic!("{}", EvalType::Int); },
                EvalType::Real => { panic!("{}", EvalType::Real); },
                EvalType::Double => { panic!("{}", EvalType::Double); },
                EvalType::Other => unreachable!(),
            }
        "#;
        let expect_output_stream: TokenStream = expect_output.parse().unwrap();

        let mt: MatchTemplate = syn::parse_str(input).unwrap();
        let output = mt.expand();
        assert_eq!(output.to_string(), expect_output_stream.to_string());
    }

    #[test]
    fn test_wildcard() {
        let input = r#"
            TT = [Foo, Bar],
            match v {
                VectorValue::TT => EvalType::TT,
                _ => unreachable!(),
            }
        "#;

        let expect_output = r#"
            match v {
                VectorValue::Foo => EvalType::Foo,
                VectorValue::Bar => EvalType::Bar,
                _ => unreachable!(),
            }
        "#;
        let expect_output_stream: TokenStream = expect_output.parse().unwrap();

        let mt: MatchTemplate = syn::parse_str(input).unwrap();
        let output = mt.expand();
        assert_eq!(output.to_string(), expect_output_stream.to_string());
    }

    #[test]
    fn test_map() {
        let input = r#"
            TT = [Foo, Bar => Baz, Bark => <&'static Whooh>()],
            match v {
                VectorValue::TT => EvalType::TT,
                EvalType::Other => unreachable!(),
            }
        "#;

        let expect_output = r#"
            match v {
                VectorValue::Foo => EvalType::Foo,
                VectorValue::Bar => EvalType::Baz,
                VectorValue::Bark => EvalType:: < & 'static Whooh>(),
                EvalType::Other => unreachable!(),
            }
        "#;
        let expect_output_stream: TokenStream = expect_output.parse().unwrap();

        let mt: MatchTemplate = syn::parse_str(input).unwrap();
        let output = mt.expand();
        assert_eq!(output.to_string(), expect_output_stream.to_string());
    }

    #[test]
    fn test_pair_map() {
        let input = r#"
            (VN, VT) = [Databases => DatabasesView, Schemas => SchemasView],
            match table_name {
                VT::TABLE_NAME => Some(SystemView::VN(VT)),
                _ => None,
            }
        "#;

        let expect_output = r#"
            match table_name {
                DatabasesView::TABLE_NAME => Some(SystemView::Databases(DatabasesView)),
                SchemasView::TABLE_NAME => Some(SystemView::Schemas(SchemasView)),
                _ => None,
            }
        "#;
        let expect_output_stream: TokenStream = expect_output.parse().unwrap();

        let mt: MatchTemplate = syn::parse_str(input).unwrap();
        let output = mt.expand();
        assert_eq!(output.to_string(), expect_output_stream.to_string());
    }

    #[test]
    fn test_pair_identical() {
        let input = r#"
            (VN, VT) = [Foo, Bar],
            match v {
                VN => VT,
            }
        "#;

        let expect_output = r#"
            match v {
                Foo => Foo,
                Bar => Bar,
            }
        "#;
        let expect_output_stream: TokenStream = expect_output.parse().unwrap();

        let mt: MatchTemplate = syn::parse_str(input).unwrap();
        let output = mt.expand();
        assert_eq!(output.to_string(), expect_output_stream.to_string());
    }
}
