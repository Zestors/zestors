use futures::future::join_all;
use zestors::core::*;
use zestors::distr::*;

use std::time::Duration;

static E8080: u32 = 0;
static E8081: u32 = 1;
static E8082: u32 = 2;

#[tokio::test]
async fn test() {
    env_logger::init();
    let (child8080, system8080) = Endpoint::spawn_insecure("127.0.0.1:8080", E8080).await.unwrap();
    let (child8081, system8081) = Endpoint::spawn_insecure("127.0.0.1:8081", E8081).await.unwrap();
    let (child8082, system8082) = Endpoint::spawn_insecure("127.0.0.1:8082", E8082).await.unwrap();


    let res = system8080.connect_insecure("127.0.0.1:8081").await.unwrap();
    let res = system8082.connect_insecure("127.0.0.1:8080").await.unwrap();
    let res = system8081.connect_insecure("127.0.0.1:8082").await.unwrap();


    drop(child8080);

    tokio::time::sleep(Duration::from_millis(1_000)).await;
}
