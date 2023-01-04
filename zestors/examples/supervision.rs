fn main() {}

use tiny_actor::Config;
use zestors::{
    process::Inbox,
    supervision::{RestartStrategy, Restartable, SimpleChildSpec, SupervisorBuilder, RawStarter},
    *,
};

async fn test<R: Restartable<Exit = (), ActorType = ()>>() {
    let _ = SupervisorBuilder::new()
        .add_spec(RawStarter::new(async move {
            let res: Result<(_, R), _> = todo!();
            res
        }))
        .add_spec(SimpleChildSpec::new(
            my_actor,
            Config::default(),
            RestartStrategy::NormalExitOnly,
        ))
        .start()
        .await
        .unwrap();
}

fn my_actor(inbox: Inbox<()>) -> () {
    ()
}
