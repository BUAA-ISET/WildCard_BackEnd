use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Expr, parse_macro_input};

#[proc_macro_derive(StatusCode, attributes(status))]
pub fn derive_status_code(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let variants = match &input.data {
        Data::Enum(e) => &e.variants,
        _ => panic!("Only use for enums"),
    };

    let arms = variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        let status_attrs: Vec<_> = variant
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("status"))
            .collect();
        match status_attrs.len() {
            0 => panic!("每个变体必须标注 #[status(...)]"),
            1 => (),
            _ => panic!("每个变体只能标注一个 #[status]"),
        };
        let status_attr = status_attrs.first().unwrap();
        let expr: Expr = status_attr.parse_args().expect("Cannot parse #[status]");

        // 自动匹配变体并返回常量
        quote! { #name::#variant_name { .. } => #expr }
    });

    let expanded = quote! {
        impl #name {
            pub fn status_code(&self) -> axum::http::StatusCode {
                match self {
                    #(#arms),*
                }
            }
        }
    };
    expanded.into()
}
