// use futures::stream::StreamExt;
// use std::time::Duration;
// use zestors_core::*;

// #[tokio::main]
// async fn main() {
//     // First we spawn an actor with a custom config, and an inbox which receives u32 messages.
//     // This will spawn 3 processes, with i = {0, 1, 2}.
//     let (mut pool, address) = spawn_pool_processes(
//         0..3,
//         Config {
//             link: Link::Attached(Duration::from_secs(1)),
//             capacity: Capacity::Unbounded(BackPressure::exponential(
//                 5,
//                 Duration::from_nanos(25),
//                 1.3,
//             )),
//         },
//         |i, mut inbox: Inbox<u32>| async move {
//             loop {
//                 // Now every actor loops in the same way as in the basic example
//                 match inbox.recv().await {
//                     Ok(msg) => println!("Received message on actor {i}: {msg}"),
//                     Err(error) => match error {
//                         RecvError::Halted => {
//                             println!("actor has received halt signal - Exiting now...");
//                             break "Halt";
//                         }
//                         RecvError::ClosedAndEmpty => {
//                             println!("Channel is closed - Exiting now...");
//                             break "Closed";
//                         }
//                     },
//                 }
//             }
//         },
//     );

//     tokio::time::sleep(Duration::from_millis(10)).await;

//     // Send it the numbers 0..10, they will be spread across all processes.
//     for num in 0..10 {
//         address.send(num).await.unwrap()
//     }

//     // And finally shut the actor down, giving it 1 second before aborting.
//     let exits = pool
//         .shutdown(Duration::from_secs(1))
//         .collect::<Vec<_>>() // Await all processes (using `futures::StreamExt::collect`)
//         .await;

//     // And assert that every exit is `Ok("Halt")`
//     for exit in exits {
//         match exit {
//             Ok(exit) => {
//                 assert_eq!(exit, "Halt");
//                 println!("actor exited with message: {exit}")
//             }
//             Err(error) => match error {
//                 ExitError::Panic(_) => todo!(),
//                 ExitError::Abort => todo!(),
//             },
//         }
//     }
// }

fn main() {}
