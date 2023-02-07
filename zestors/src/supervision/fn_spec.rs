use super::*;
use pin_project::pin_project;

//------------------------------------------------------------------------------------------------
//  Spec
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct MapRefSpec<Sp, Fun, T>
where
    Sp: Spec,
    Fun: FnMut(Sp::Ref) -> T,
{
    #[pin]
    spec: Sp,
    map: Option<Fun>,
}

impl<Sp, Fun, T> MapRefSpec<Sp, Fun, T>
where
    Sp: Spec,
    Fun: FnMut(Sp::Ref) -> T,
{
    pub fn new(spec: Sp, map: Fun) -> Self {
        Self {
            spec,
            map: Some(map),
        }
    }
}

impl<Sp, Fun, T> Spec for MapRefSpec<Sp, Fun, T>
where
    Sp: Spec,
    Fun: FnMut(Sp::Ref) -> T,
{
    type Ref = T;
    type Supervisee = MapRefSupervisee<Sp::Supervisee, Fun, T>;

    fn poll_start(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeStart<(Self::Supervisee, Self::Ref), Self>> {
        let proj = self.project();
        proj.spec.poll_start(cx).map(|start| {
            start
                .map_failure(|spec| MapRefSpec {
                    spec,
                    map: Some(proj.map.take().unwrap()),
                })
                .map_success(|(supervisee, reference)| {
                    let mut map = proj.map.take().unwrap();
                    let reference = map(reference);
                    (
                        MapRefSupervisee {
                            supervisee,
                            map: Some(map),
                        },
                        reference,
                    )
                })
        })
    }

    fn start_timeout(self: Pin<&Self>) -> Duration {
        self.project_ref().spec.start_timeout()
    }
}

//------------------------------------------------------------------------------------------------
//  Supervisee
//------------------------------------------------------------------------------------------------

#[pin_project]
pub struct MapRefSupervisee<S, Fun, T>
where
    S: Supervisee,
    Fun: FnMut(<S::Spec as Spec>::Ref) -> T,
{
    #[pin]
    supervisee: S,
    map: Option<Fun>,
}

impl<S, Fun, T> Supervisee for MapRefSupervisee<S, Fun, T>
where
    S: Supervisee,
    Fun: FnMut(<S::Spec as Spec>::Ref) -> T,
{
    type Spec = MapRefSpec<S::Spec, Fun, T>;

    fn poll_supervise(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<SuperviseeExit<Self::Spec>> {
        let proj = self.project();
        proj.supervisee.poll_supervise(cx).map(|res| {
            res.map_exit(|spec| MapRefSpec {
                spec,
                map: proj.map.take(),
            })
        })
    }

    fn abort_timeout(self: Pin<&Self>) -> Duration {
        self.project_ref().supervisee.abort_timeout()
    }

    fn halt(self: Pin<&mut Self>) {
        self.project().supervisee.halt()
    }

    fn abort(self: Pin<&mut Self>) {
        self.project().supervisee.abort()
    }
}
