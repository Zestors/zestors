use uuid::Uuid;
use zestors::distributed::{
    local_node::{self, LocalNode},
    node,
};

#[tokio::main]
pub async fn main() {
    let cookie1 = Uuid::new_v4();
    let cookie2 = Uuid::new_v4();

    let local_node_1 = LocalNode::initialize(
        cookie1,
        "hi".to_string(),
        "127.0.0.1:1234".parse().unwrap(),
    ).await.unwrap();

    let local_node_2 = LocalNode::initialize(
        cookie1,
        "hi".to_string(),
        "127.0.0.1:1235".parse().unwrap(),
    ).await.unwrap();

    let res = local_node_1.connect("127.0.0.1:1235".parse().unwrap()).await;
    // let res = local_node_2.connect_to_node("127.0.0.1:1234".parse().unwrap()).await;

    println!("connect_result: {:?}", res);

    // println!("{:?}", local_node_1);
    // println!("{:?}", local_node_2);
}
