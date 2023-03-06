use super::Specification;

pub struct Supervisor<S> {
    specification: S
}

// impl<S: Specification> Supervisor<S> {
//     pub async fn spawn(self) ->
// }