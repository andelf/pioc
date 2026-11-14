//! Compile-time PIOC assembly macros.

use std::path::PathBuf;

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, LitStr, Token};

fn words(values: &[u16]) -> TokenStream {
    quote!({
        const WORDS: &[u16] = &[#(#values),*];
        WORDS
    })
    .into()
}

fn diagnostic(span: &LitStr, error: impl std::fmt::Display) -> TokenStream {
    syn::Error::new_spanned(span, error.to_string())
        .to_compile_error()
        .into()
}

/// Assemble WCH-compatible PIOC source at compile time.
///
/// Each string is one or more source lines:
///
/// ```ignore
/// const CODE: &[u16] = pioc_proc::pioc_asm!(
///     "ORG 0",
///     "DW 0",
///     "MOVL 0x55",
/// );
/// ```
#[proc_macro]
pub fn pioc_asm(input: TokenStream) -> TokenStream {
    assemble_inline(input, false)
}

/// Assemble canonical PIOC source at compile time.
#[proc_macro]
pub fn pioc_asm_canonical(input: TokenStream) -> TokenStream {
    assemble_inline(input, true)
}

fn assemble_inline(input: TokenStream, canonical: bool) -> TokenStream {
    let parser = Punctuated::<LitStr, Token![,]>::parse_terminated;
    let lines = match parser.parse(input) {
        Ok(lines) => lines,
        Err(error) => return error.to_compile_error().into(),
    };
    let mut source = String::new();
    for line in &lines {
        source.push_str(&line.value());
        source.push('\n');
    }
    let program = if canonical {
        pioc_parser::assemble_canonical(&source)
    } else {
        pioc_parser::assemble_legacy(&source)
    };
    match program {
        Ok(program) => words(&program.words),
        Err(error) => {
            if let Some(first) = lines.first() {
                diagnostic(first, error)
            } else {
                syn::Error::new(proc_macro2::Span::call_site(), error.to_string())
                    .to_compile_error()
                    .into()
            }
        }
    }
}

/// Assemble a legacy PIOC file relative to the invoking crate's manifest.
#[proc_macro]
pub fn pioc_file(input: TokenStream) -> TokenStream {
    assemble_file(input, pioc_parser::Syntax::Legacy)
}

/// Assemble a canonical PIOC file relative to the invoking crate's manifest.
#[proc_macro]
pub fn pioc_file_canonical(input: TokenStream) -> TokenStream {
    assemble_file(input, pioc_parser::Syntax::Canonical)
}

fn assemble_file(input: TokenStream, syntax: pioc_parser::Syntax) -> TokenStream {
    let path = parse_macro_input!(input as LitStr);
    let relative = path.value();
    let mut absolute =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| ".".into()));
    absolute.push(&relative);
    match pioc_parser::assemble_file_with_dependencies(&absolute, syntax) {
        Ok((program, dependencies)) => {
            let values = program.words;
            let dependencies = dependencies
                .iter()
                .map(|dependency| dependency.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            quote!({
                const _: &[&str] = &[#(include_str!(#dependencies)),*];
                const WORDS: &[u16] = &[#(#values),*];
                WORDS
            })
            .into()
        }
        Err(error) => diagnostic(&path, error),
    }
}
