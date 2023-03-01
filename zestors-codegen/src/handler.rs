use proc_macro2::TokenStream;
use syn::{parse2, Error, Item, Type};

pub fn derive_handler(item: TokenStream) -> Result<TokenStream, Error> {
    let (ident, generics, attrs) = match parse2::<Item>(item)? {
        Item::Enum(item) => (item.ident, item.generics, item.attrs),
        Item::Struct(item) => (item.ident, item.generics, item.attrs),
        Item::Union(item) => (item.ident, item.generics, item.attrs),
        item => Err(Error::new_spanned(item, "Must be an Enum, Struct or Union"))?,
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let Some(state) = attrs.iter().find(|attr| attr.path.is_ident("state")) else {
        return Err(Error::new_spanned(ident, "No #[state(..)] attribute found"))?
    };
    let state = state.parse_args::<Type>()?;

    Ok(quote! {
        #[zestors::export::async_trait]
        impl #impl_generics zestors::handler::Handler for #ident #ty_generics #where_clause
        {
            type State = #state;
            type Exception = zestors::export::Report;
            type Stop = ();
            type Exit = Result<Self, zestors::export::Report>;

            async fn handle_exit(self,
                state: &mut Self::State,
                event: zestors::handler::Event<Self>
            ) -> zestors::handler::ExitFlow<Self> {
                match event {
                    zestors::handler::Event::Halted => {
                        state.close();
                        zestors::handler::ExitFlow::Continue(self)
                    }
                    zestors::handler::Event::ClosedAndEmpty => zestors::handler::ExitFlow::Exit(Ok(self)),
                    zestors::handler::Event::Dead => zestors::handler::ExitFlow::Exit(Ok(self)),
                    zestors::handler::Event::Exception(exception) => zestors::handler::ExitFlow::Exit(Err(exception)),
                    zestors::handler::Event::Stop(()) => zestors::handler::ExitFlow::Exit(Ok(self)),
                }
            }
        }
    })
}
