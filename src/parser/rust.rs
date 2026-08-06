//! Rust source parser.
//!
//! Extracts top-level symbols from Rust source code using the `syn` crate.

use crate::model::{Node, NodeId, NodeKind};
use quote::quote;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, ExprLit, ImplItem, ImplItemFn, Item, ItemFn, Lit, Meta, MetaNameValue};

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
                    let doc = Self::extract_docs(&item_struct.attrs);
                    let mut decl = item_struct.clone();
                    decl.attrs.clear();
                    for field in decl.fields.iter_mut() {
                        field.attrs.clear();
                    }
                    nodes.push(Node {
                        id: NodeId(id_counter),
                        kind: NodeKind::Struct,
                        signature: format!("{}", quote! { #decl }),
                        fqn: name.clone(),
                        name,
                        file: file.to_string(),
                        line_start: item_struct.span().start().line,
                        line_end: item_struct.span().end().line,
                        doc,
                    });
                    id_counter += 1;
                }
                Item::Enum(item_enum) => {
                    let name = item_enum.ident.to_string();
                    let doc = Self::extract_docs(&item_enum.attrs);
                    let mut decl = item_enum.clone();
                    decl.attrs.clear();
                    for variant in decl.variants.iter_mut() {
                        variant.attrs.clear();
                        for field in variant.fields.iter_mut() {
                            field.attrs.clear();
                        }
                    }
                    nodes.push(Node {
                        id: NodeId(id_counter),
                        kind: NodeKind::Enum,
                        signature: format!("{}", quote! { #decl }),
                        fqn: name.clone(),
                        name,
                        file: file.to_string(),
                        line_start: item_enum.span().start().line,
                        line_end: item_enum.span().end().line,
                        doc,
                    });
                    id_counter += 1;
                }
                Item::Trait(item_trait) => {
                    let name = item_trait.ident.to_string();
                    let doc = Self::extract_docs(&item_trait.attrs);
                    let mut decl = item_trait.clone();
                    decl.attrs.clear();
                    for trait_item in &mut decl.items {
                        match trait_item {
                            syn::TraitItem::Fn(f) => {
                                f.attrs.clear();
                                f.default = None;
                            }
                            syn::TraitItem::Const(c) => c.attrs.clear(),
                            syn::TraitItem::Type(t) => t.attrs.clear(),
                            _ => {}
                        }
                    }
                    nodes.push(Node {
                        id: NodeId(id_counter),
                        kind: NodeKind::Trait,
                        signature: format!("{}", quote! { #decl }),
                        fqn: name.clone(),
                        name,
                        file: file.to_string(),
                        line_start: item_trait.span().start().line,
                        line_end: item_trait.span().end().line,
                        doc,
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

impl RustParser {
    pub fn parse_calls(source: &str) -> Vec<(String, String)> {
        let ast = match syn::parse_file(source) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let mut visitor = CallVisitor::default();
        visitor.visit_file(&ast);
        visitor.calls
    }

    /// Extract `(type_name, trait_name)` pairs from `impl Trait for Type` blocks.
    pub fn parse_impls(source: &str) -> Vec<(String, String)> {
        let ast = match syn::parse_file(source) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        for item in ast.items {
            if let Item::Impl(item_impl) = item {
                if let Some((_, trait_path, _)) = &item_impl.trait_ {
                    let Some(trait_seg) = trait_path.segments.last() else {
                        continue;
                    };
                    if let Some(type_name) = type_ident(&item_impl.self_ty) {
                        out.push((type_name, trait_seg.ident.to_string()));
                    }
                }
            }
        }
        out
    }

    /// Extract `(owner, type)` pairs: each symbol and the named types it references
    /// in its signature or fields. Feeds `Uses` edges so types gain centrality.
    pub fn parse_uses(source: &str) -> Vec<(String, String)> {
        let ast = match syn::parse_file(source) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        for item in &ast.items {
            match item {
                Item::Fn(f) => collect_sig_uses(&f.sig.ident.to_string(), &f.sig, &mut out),
                Item::Impl(item_impl) => {
                    let self_ty = type_ident(&item_impl.self_ty);
                    for impl_item in &item_impl.items {
                        if let ImplItem::Fn(m) = impl_item {
                            let owner = m.sig.ident.to_string();
                            if let Some(ty) = &self_ty {
                                out.push((owner.clone(), ty.clone()));
                            }
                            collect_sig_uses(&owner, &m.sig, &mut out);
                        }
                    }
                }
                Item::Struct(s) => {
                    let owner = s.ident.to_string();
                    for field in &s.fields {
                        collect_type_idents(&field.ty, &mut |t| out.push((owner.clone(), t)));
                    }
                }
                Item::Enum(e) => {
                    let owner = e.ident.to_string();
                    for variant in &e.variants {
                        for field in &variant.fields {
                            collect_type_idents(&field.ty, &mut |t| out.push((owner.clone(), t)));
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }
}

/// The final path segment ident of a named type, if it is one.
fn type_ident(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// Collect the type idents referenced in a function signature (params + return).
fn collect_sig_uses(owner: &str, sig: &syn::Signature, out: &mut Vec<(String, String)>) {
    for input in &sig.inputs {
        if let syn::FnArg::Typed(pat) = input {
            collect_type_idents(&pat.ty, &mut |t| out.push((owner.to_string(), t)));
        }
    }
    if let syn::ReturnType::Type(_, ty) = &sig.output {
        collect_type_idents(ty, &mut |t| out.push((owner.to_string(), t)));
    }
}

/// Recursively collect the last-segment idents of every named type inside `ty`,
/// descending through references, containers and generic arguments.
fn collect_type_idents(ty: &syn::Type, f: &mut impl FnMut(String)) {
    match ty {
        syn::Type::Path(tp) => {
            if let Some(seg) = tp.path.segments.last() {
                f(seg.ident.to_string());
                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner) = arg {
                            collect_type_idents(inner, f);
                        }
                    }
                }
            }
        }
        syn::Type::Reference(r) => collect_type_idents(&r.elem, f),
        syn::Type::Slice(s) => collect_type_idents(&s.elem, f),
        syn::Type::Array(a) => collect_type_idents(&a.elem, f),
        syn::Type::Tuple(t) => {
            for elem in &t.elems {
                collect_type_idents(elem, f);
            }
        }
        syn::Type::Paren(p) => collect_type_idents(&p.elem, f),
        syn::Type::Group(g) => collect_type_idents(&g.elem, f),
        _ => {}
    }
}

#[derive(Default)]
struct CallVisitor {
    scope: Vec<String>,
    calls: Vec<(String, String)>,
}

impl<'ast> Visit<'ast> for CallVisitor {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.scope.push(node.sig.ident.to_string());
        visit::visit_item_fn(self, node);
        self.scope.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.scope.push(node.sig.ident.to_string());
        visit::visit_impl_item_fn(self, node);
        self.scope.pop();
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let (Some(caller), Expr::Path(path)) = (self.scope.last(), &*node.func) {
            if let Some(seg) = path.path.segments.last() {
                self.calls.push((caller.clone(), seg.ident.to_string()));
            }
        }
        visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if let Some(caller) = self.scope.last() {
            self.calls.push((caller.clone(), node.method.to_string()));
        }
        visit::visit_expr_method_call(self, node);
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
        assert!(st.signature.contains("field"));

        let en = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Enum)
            .expect("enum node");
        assert_eq!(en.name, "MyEnum");
        assert!(en.signature.contains('A'));

        let tr = nodes
            .iter()
            .find(|n| n.kind == NodeKind::Trait)
            .expect("trait node");
        assert_eq!(tr.name, "MyTrait");
        assert!(tr.signature.contains("trait_method"));

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
