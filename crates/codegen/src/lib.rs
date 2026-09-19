//! Macros for the `zestors` crate.

extern crate proc_macro;
use darling::FromAttributes;
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Type};

/// Derives the `Interface` trait for an enum.
///
/// This only works un unnamed, single-value enum-fields that are envelopes.
/// Anything else is rejected.
///
/// # Example
/// ```
/// # use zestors_interface::*;
/// #[derive(Interface)]
/// # #[zestors(interface_path = "zestors_interface")]
/// enum MyInterface {
///     MessageA(Envelope<u32>),
///     MessageB(Envelope<String>),
/// }
/// ```
#[proc_macro_derive(Interface, attributes(zestors))]
pub fn derive_interface_polybox(input: TokenStream) -> TokenStream {
    derive_interface(input)
}

/// The attributes shared by all derives: `#[zestors(interface_path = "..",
/// distr_path = "..", actor_path = "..")]` override where the generated code
/// finds each crate, and `#[msg(reply = .., id = "..")]` configure messages.
#[derive(darling::FromAttributes)]
#[darling(attributes(zestors, msg))]
struct Attrs {
    interface_path: Option<syn::Path>,
    distr_path: Option<syn::Path>,
    actor_path: Option<syn::Path>,
    reply: Option<syn::Type>,
    id: Option<syn::LitStr>,
    no_auto_register: darling::util::Flag,
}

impl Attrs {
    fn interface_path(&self) -> syn::Path {
        self.interface_path
            .clone()
            .unwrap_or_else(|| syn::parse_str("::zestors::interface").unwrap())
    }

    fn distr_path(&self) -> syn::Path {
        self.distr_path
            .clone()
            .unwrap_or_else(|| syn::parse_str("::zestors::distr").unwrap())
    }

    fn actor_path(&self) -> syn::Path {
        self.actor_path
            .clone()
            .unwrap_or_else(|| syn::parse_str("::zestors::actor").unwrap())
    }
}

fn derive_interface(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let attrs = match Attrs::from_attributes(&input.attrs) {
        Ok(attrs) => attrs,
        Err(err) => return err.write_errors().into(),
    };
    let enum_name = &input.ident;

    let base_path: syn::Path = attrs.interface_path();
    let msg_path: syn::Path = base_path.clone();

    // Ensure we are working with an enum
    let variants = match &input.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => panic!("Interface derive can only be used on enums"),
    };

    let mut inner_types = Vec::new();
    let mut try_from_matches = Vec::new();
    let mut try_into_matches = Vec::new();
    let mut into_matches = Vec::new();
    let mut from_impls = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field_type = &fields.unnamed[0].ty;

                let inner_type = extract_inner_envelope_type(field_type)
                    .expect("Interface variants must be of type Envelope<T>");

                inner_types.push(inner_type);

                try_from_matches.push(quote! {
                    let envelope = match envelope.downcast::<#inner_type>() {
                        Ok(envelope) => return Ok(Self::#variant_name(envelope)),
                        Err(envelope) => envelope,
                    };
                });

                try_into_matches.push(quote! {
                    if id == std::any::TypeId::of::<#inner_type>() {
                        if let Self::#variant_name(envelope) = self {
                            // SAFETY: Verified type matches dynamic I parameter.
                            let converted = unsafe {
                                std::mem::transmute_copy::<#base_path::Envelope<#inner_type>, #base_path::Envelope<I>>(&envelope)
                            };
                            std::mem::forget(envelope);
                            return Ok(converted);
                        }
                    }
                });

                into_matches.push(quote! {
                    Self::#variant_name(envelope) => #msg_path::AnyEnvelope::new::<#inner_type>(envelope),
                });

                from_impls.push(quote! {
                    impl From<#msg_path::Envelope<#inner_type>> for #enum_name {
                        fn from(envelope: #msg_path::Envelope<#inner_type>) -> Self {
                            Self::#variant_name(envelope)
                        }
                    }

                    impl TryInto<#msg_path::Envelope<#inner_type>> for #enum_name {
                        type Error = Self;

                        fn try_into(self) -> Result<#msg_path::Envelope<#inner_type>, Self> {
                            if let #enum_name::#variant_name(envelope) = self {
                                Ok(envelope)
                            } else {
                                Err(self)
                            }
                        }
                    }
                });
            }
            _ => panic!("Interface derive only supports variants with a single unnamed field, e.g., A(Envelope<T>)"),
        }
    }

    let expanded = quote! {
        impl #msg_path::Interface for #enum_name {
            fn try_from_dyn_envelope(envelope: #msg_path::AnyEnvelope) -> Result<Self, #msg_path::AnyEnvelope> {
                #(#try_from_matches)*
                Err(envelope)
            }

            // Could be added to improve performance, but would require unsafe transmute to avoid double downcasting.
            // fn try_into_envelope<I: #base_path::Message>(self) -> Result<#base_path::Envelope<I>, Self> {
            //     let id = std::any::TypeId::of::<I>();
            //     #(#try_into_matches)*
            //     Err(self)
            // }

            fn into_dyn_envelope(self) -> #msg_path::AnyEnvelope {
                match self {
                    #(#into_matches)*
                }
            }

            type Set = (#(#inner_types,)*);
        }


        impl #msg_path::Message for #enum_name {
            type Output = ();
            type Kind = #msg_path::Cast;
        }


        impl From<#msg_path::Envelope<#enum_name>> for #enum_name {
            fn from(envelope: #msg_path::Envelope<#enum_name>) -> Self {
                envelope.msg
            }
        }

        impl TryInto<#msg_path::Envelope<#enum_name>> for #enum_name {
            type Error = Self;

            fn try_into(self) -> Result<#msg_path::Envelope<#enum_name>, Self> {
                Ok(#msg_path::Envelope::new(self, ()))
            }
        }

        #(#from_impls)*
    };

    TokenStream::from(expanded)
}

/// Derives the `HandlerInterface` trait for an enum, allowing it to act as a
/// handler interface for an actor. Each variant must contain a single unnamed
/// field of type `Envelope<T>`.
#[proc_macro_derive(HandlerInterface, attributes(zestors))]
pub fn derive_actor_interface(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let enum_name = &input.ident;
    let attrs = match Attrs::from_attributes(&input.attrs) {
        Ok(attrs) => attrs,
        Err(err) => return err.write_errors().into(),
    };

    // Ensure we are working with an enum
    let variants = match &input.data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => panic!("HandlerInterface derive can only be used on enums"),
    };

    let actor_path = attrs.actor_path();

    let mut handle_matches = Vec::new();
    let mut inner_types = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;

        match &variant.fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field_type = &fields.unnamed[0].ty;

                let inner_type = extract_inner_envelope_type(field_type)
                    .expect("HandlerInterface variants must be of type Envelope<T>");

                handle_matches.push(quote! {
                    Self::#variant_name(envelope) => {
                        <T as #actor_path::Handle<#inner_type>>::handle(actor, state, envelope.msg, envelope.req).await
                    }
                });
                inner_types.push(inner_type);
            }
            _ => panic!("HandlerInterface derive only supports variants with a single unnamed field, e.g., A(Envelope<T>)"),
        }
    }

    let expanded = quote! {
        impl<T> #actor_path::HandlerInterface<T> for #enum_name
        where
            T: #actor_path::Handler + #( #actor_path::Handle<#inner_types> + )*
        {
            async fn handle_with(self, state: #actor_path::HandlerContext<'_, T>, actor: &mut T) -> Result<(), ::rootcause::Report> {
                match self {
                    #(#handle_matches)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}

/// Derives the `Message` trait.
///
/// This macro accepts an optional `reply` attribute to specify the reply type for the message.
/// When `reply` is specified, the receipt becomes a `Response` and the resolver a `Request`.
///
///
/// # Example
/// ```
/// # use zestors_interface::*;
/// #[derive(Message)]
/// # #[zestors(interface_path = "zestors_interface")]
/// struct SimpleMessage;
///
/// #[derive(Message)]
/// #[msg(reply = u32)]
/// # #[zestors(interface_path = "zestors_interface")]
/// struct MessageWithOutput;
/// ```
#[proc_macro_derive(Message, attributes(msg, zestors))]
pub fn derive_message(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let attrs = match Attrs::from_attributes(&input.attrs) {
        Ok(attrs) => attrs,
        Err(err) => return err.write_errors().into(),
    };
    let name = &input.ident;

    let base_path: syn::Path = attrs.interface_path();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let expanded = if let Some(reply_type) = attrs.reply {
        quote!(
            impl #impl_generics #base_path::Message for #name #ty_generics #where_clause
            {
                type Output = #reply_type;
                type Kind = #base_path::Call;
            }
        )
    } else {
        quote!(
            impl #impl_generics #base_path::Message for #name #ty_generics #where_clause
            {
                type Output = ();
                type Kind = #base_path::Cast;
            }
        )
    };

    TokenStream::from(expanded)
}

/// Derives the `StableId` trait, giving the message a stable, globally unique
/// [`MessageId`](https://docs.rs/zestors-distr) that identifies it across nodes.
///
/// The id is set with `#[msg(id = "<uuid>")]`. If it is missing, compilation
/// fails with a freshly generated random id that can be pasted in.
///
/// With the `auto-register` feature of `zestors-distr`, a type that isn't
/// generic is also collected for `Cluster::auto_register`, which registers it
/// if it is a `RemoteMessage`. `#[msg(no_auto_register)]` leaves it out.
///
/// # Example
/// ```
/// # use zestors_distr::*;
/// #[derive(StableId)]
/// # #[zestors(distr_path = "zestors_distr")]
/// #[msg(id = "0b0f7e4e-3f3a-4d5b-9d7e-6a1c2b3d4e92")]
/// struct Ping;
///
/// assert_eq!(Ping::Id, MessageId::from_u128(0x0b0f7e4e_3f3a_4d5b_9d7e_6a1c2b3d4e92));
/// ```
#[proc_macro_derive(StableId, attributes(msg, zestors))]
pub fn derive_message_id(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let attrs = match Attrs::from_attributes(&input.attrs) {
        Ok(attrs) => attrs,
        Err(err) => return err.write_errors().into(),
    };
    let name = &input.ident;
    let distr_path = attrs.distr_path();

    let Some(id) = attrs.id else {
        let suggestion = uuid::Uuid::new_v4();
        return syn::Error::new_spanned(
            name,
            format!(
                "`StableId` requires a unique id; add this attribute to the type: #[msg(id = \"{suggestion}\")]"
            ),
        )
        .to_compile_error()
        .into();
    };

    let uuid = match uuid::Uuid::parse_str(&id.value()) {
        Ok(uuid) => uuid.as_u128(),
        Err(err) => {
            return syn::Error::new(Span::call_site(), format!("invalid message id: {err}"))
                .to_compile_error()
                .into();
        }
    };

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // A generic type has no one type to register.
    let auto_register = (input.generics.params.is_empty() && !attrs.no_auto_register.is_present())
        .then(|| quote!(#distr_path::__auto_register!(#name);));

    TokenStream::from(quote! {
        impl #impl_generics #distr_path::StableId for #name #ty_generics #where_clause {
            const Id: #distr_path::MessageId = #distr_path::MessageId::from_u128(#uuid);
        }
        #auto_register
    })
}

fn extract_inner_envelope_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        let segment = type_path.path.segments.last()?;
        if segment.ident == "Envelope" {
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                    return Some(inner_ty);
                }
            }
        }
    }
    None
}
