use std::{convert::TryFrom, net::SocketAddr, time::Duration};

use zestors_distributed::SystemRef;

#[tokio::test]
pub async fn test() {
    let (child, system_8080) = SystemRef::spawn("127.0.0.1:8080".parse().unwrap(), 8080)
        .await
        .unwrap();
    let (child, system_8081) = SystemRef::spawn("127.0.0.1:8081".parse().unwrap(), 8081)
        .await
        .unwrap();
    let (child, system_8082) = SystemRef::spawn("127.0.0.1:8082".parse().unwrap(), 8082)
        .await
        .unwrap();

    system_8080
        .connect("127.0.0.1:8081".parse().unwrap())
        .await
        .unwrap();
    system_8082
        .connect("127.0.0.1:8081".parse().unwrap())
        .await
        .unwrap();

    child.halt();
    tokio::time::sleep(Duration::from_secs(10)).await;
}
