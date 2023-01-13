macro_rules! inline_docs {
    ($($item:item)*) => {
        $(
            #[doc(inline)]
            $item
        )*
    };
}
pub(crate) use inline_docs;