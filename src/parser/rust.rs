//! Rust source parser.
//!
//! Extracts top-level symbols from Rust source code using the `syn` crate.

use crate::model::{Node, NodeId, NodeKind};
use quote::quote;
use syn::spanned::Spanned;
use syn::{Attribute, Expr, ExprLit, ImplItem, ImplItemFn, Item, Lit, Meta, MetaNameValue};

/// Parser for Rust source code.
pub struct RustParser;

impl RustParser {
    /// Collect the text of leading `///` doc comments, if any.
    fn extract_docs(attrs: &[Attribute]) -> Option<String> {
        let mut docs = Vec::new();
        for attr in attrs {
            if attr.path().is_ident("doc") {
                if let Meta::NameValue(MetaNameValue {
                    value:
                        Expr::Lit(ExprLit {
                            lit: Lit::Str(s), ..
                        }),
                    ..
                }) = &attr.meta
                {
                    docs.push(s.value().trim().to_string());
                }
            }
        }
        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
        }
    }
}

impl super::LanguageParser for RustParser {
    fn parse_source(&self, source: &str, file: &str) -> Result<Vec<Node>, super::ParseError> {
        let ast = syn::parse_file(source).map_err(|e| super::ParseError::Syntax(e.to_string()))?;

        let mut nodes = Vec::new();
        let mut id_counter = 0u32;

        for item in ast.items {
            match item {
                Item::Fn(item_fn) => {
                    let sig = &item_fn.sig;
                    let signature = format!("{}", quote! { #sig });
                    let name = item_fn.sig.ident.to_string();
                    nodes.push(Node {
                        id: NodeId(id_counter),
                        kind: NodeKind::Function,
                        fqn: name.clone(),
                        name,
                        signature,
                        file: file.to_string(),
                        line_start: item_fn.span().start().line,
                        line_end: item_fn.span().end().line,
                        doc: Self::extract_docs(&item_fn.attrs),
                    });
                    id_counter += 1;
                }
                Item::Struct(item_struct) => {
                    let name = item_struct.ident.to_string();
                    nodes.push(Node {
                        id: NodeId(id_counter),
                        kind: NodeKind::Struct,
                        signature: format!("struct {name}"),
                        fqn: name.clone(),
                        name,
                        file: file.to_string(),
                        line_start: item_struct.span().start().line,
                        line_end: item_struct.span().end().line,
                        doc: Self::extract_docs(&item_struct.attrs),
                    });
                    id_counter += 1;
                }
                Item::Enum(item_enum) => {
                    let name = item_enum.ident.to_string();
                    nodes.push(Node {
                        id: NodeId(id_counter),
                        kind: NodeKind::Enum,
                        signature: format!("enum {name}"),
                        fqn: name.clone(),
                        name,
                        file: file.to_string(),
                        line_start: item_enum.span().start().line,
                        line_end: item_enum.span().end().line,
                        doc: Self::extract_docs(&item_enum.attrs),
                    });
                    id_counter += 1;
                }
                Item::Trait(item_trait) => {
                    let name = item_trait.ident.to_string();
                    nodes.push(Node {
                        id: NodeId(id_counter),
                        kind: NodeKind::Trait,
                        signature: format!("trait {name}"),
                        fqn: name.clone(),
                        name,
                        file: file.to_string(),
                        line_start: item_trait.span().start().line,
                        line_end: item_trait.span().end().line,
                        doc: Self::extract_docs(&item_trait.attrs),
                    });
                    id_counter += 1;
                }
                Item::Mod(item_mod) => {
                    let name = item_mod.ident.to_string();
                    nodes.push(Node {
                        id: NodeId(id_counter),
                        kind: NodeKind::Module,
                        signature: format!("mod {name}"),
                        fqn: name.clone(),
                        name,
                        file: file.to_string(),
                        line_start: item_mod.span().start().line,
                        line_end: item_mod.span().end().line,
                        doc: Self::extract_docs(&item_mod.attrs),
                    });
                    id_counter += 1;
                }
                Item::Const(item_const) => {
                    let name = item_const.ident.to_string();
                    nodes.push(Node {
                        id: NodeId(id_counter),
                        kind: NodeKind::Const,
                        signature: format!("const {name}"),
                        fqn: name.clone(),
                        name,
                        file: file.to_string(),
                        line_start: item_const.span().start().line,
                        line_end: item_const.span().end().line,
                        doc: Self::extract_docs(&item_const.attrs),
                    });
                    id_counter += 1;
                }
                Item::Impl(item_impl) => {
                    let self_ty = &item_impl.self_ty;
                    let self_ty_str = format!("{}", quote! { #self_ty });
                    for impl_item in item_impl.items {
                        if let ImplItem::Fn(ImplItemFn { sig, attrs, .. }) = impl_item {
                            let signature = format!("{}", quote! { #sig });
                            let name = sig.ident.to_string();
                            nodes.push(Node {
                                id: NodeId(id_counter),
                                kind: NodeKind::Method,
                                fqn: format!("{self_ty_str}::{name}"),
                                name,
                                signature,
                                file: file.to_string(),
                                line_start: sig.span().start().line,
                                line_end: sig.span().end().line,
                                doc: Self::extract_docs(&attrs),
                            });
                            id_counter += 1;
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeKind;
    use crate::parser::LanguageParser;

    const SOURCE: &str = r#"
        /// Function doc
        pub fn free_fn(x: i32) -> i32 { x }

        /// Module doc
        mod my_mod {}

        /// Struct doc
        pub struct MyStruct {
            pub field: i32,
        }

        /// Enum doc
        pub enum MyEnum {
            A,
            B,
        }

        /// Trait doc
        pub trait MyTrait {
            fn trait_method(&self);
        }

        /// Const doc
        pub const MY_CONST: i32 = 42;

        /// Impl doc
        impl MyStruct {
            /// Method doc
            pub fn method(&self) {}
        }
    "#;

    #[test]
    fn test_parse() {
        let parser = RustParser;
        let nodes = parser
            .parse_source(SOURCE, "test.rs")
            .expect("parse failed");
        assert_eq!(nodes.len(), 7);

        let func = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Function)
            .expect("function node");
        assert_eq!(func.name, "free_fn");
        assert!(func.signature.contains("fn free_fn"));

        let st = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Struct)
            .expect("struct node");
        assert_eq!(st.name, "MyStruct");

        let en = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Enum)
            .expect("enum node");
        assert_eq!(en.name, "MyEnum");

        let tr = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Trait)
            .expect("trait node");
        assert_eq!(tr.name, "MyTrait");

        let md = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Module)
            .expect("module node");
        assert_eq!(md.name, "my_mod");

        let co = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Const)
            .expect("const node");
        assert_eq!(co.name, "MY_CONST");

        let me = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Method)
            .expect("method node");
        assert_eq!(me.name, "method");
        assert!(me.fqn.contains("MyStruct::method"));
    }

    #[test]
    fn test_invalid_source() {
        let parser = RustParser;
        let res = parser.parse_source("fn bad {", "bad.rs");
        assert!(res.is_err());
    }
}
