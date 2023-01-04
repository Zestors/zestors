// use crate::*;

// //------------------------------------------------------------------------------------------------
// //  SpecifiesChild
// //------------------------------------------------------------------------------------------------

// pub trait SpecifiesChild {
//     type Startable: Specifies;

//     fn into_childspec(self) -> ChildSpec<Self::Startable>;
// }

// impl<S: Specifies> SpecifiesChild for (Link, S) {
//     type Startable = S;

//     fn into_childspec(self) -> ChildSpec<S> {
//         ChildSpec::new(self.0, self.1)
//     }
// }

// impl<S: Specifies> SpecifiesChild for (S, Link) {
//     type Startable = S;

//     fn into_childspec(self) -> ChildSpec<S> {
//         ChildSpec::new(self.1, self.0)
//     }
// }

// impl<S: Specifies> SpecifiesChild for ChildSpec<S> {
//     type Startable = S;

//     fn into_childspec(self) -> ChildSpec<S> {
//         self
//     }
// }

// //------------------------------------------------------------------------------------------------
// //  ChildSpecExt
// //------------------------------------------------------------------------------------------------

// pub trait SpecifiesChildExt: SpecifiesChild {
//     fn start(self) -> StartFut;
// }

// pub struct StartFut;

// //------------------------------------------------------------------------------------------------
// //  ChildSpec
// //------------------------------------------------------------------------------------------------

// pub struct ChildSpec<S: Specifies> {
//     link: Link,
//     startable: S,
// }

// impl<S: Specifies> ChildSpec<S> {
//     pub fn new(link: Link, startable: S) -> Self {
//         Self { link, startable }
//     }
// }

// //------------------------------------------------------------------------------------------------
// //  DynamicChildSpec
// //------------------------------------------------------------------------------------------------

// pub struct DynamicChildSpec {
//     link: Link,
//     // startable: Box<dyn Startable>
// }
