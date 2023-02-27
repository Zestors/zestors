use proc_macro2::TokenStream;
use syn::{Error, parse2, Item,};

pub fn derive_handle_exit(item: TokenStream) -> Result<TokenStream, Error> {
    let (ident, generics) = match parse2::<Item>(item)? {
        Item::Enum(item) => (item.ident, item.generics),
        Item::Struct(item) => (item.ident, item.generics),
        Item::Union(item) => (item.ident, item.generics),
        item => Err(Error::new_spanned(item, "Must be an Enum, Struct or Union"))?,
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    Ok(quote! {
        #[zestors::async_trait]
        impl #impl_generics zestors::handler::HandleExit for #ident #ty_generics #where_clause {
            type Exit = zestors::handler::ExitReason<<Self as Handler>::Exception>;

            async fn handle_exit(
                self,
                _state: &mut <Self as Handler>::State,
                reason: ExitReason<<Self as Handler>::Exception>,
            ) -> zestors::handler::ExitFlow<Self> {
                zestors::handler::ExitFlow::Exit(reason)
            }
        }
    })
}