use rootcause::Report;
use zestors_runtime::prelude::*;

pub async fn simplest_handler(mut inbox: Inbox<()>) -> Result<(), Report> {
    while let Some(msg) = inbox.next().await {
        println!("Msg received: {msg:?}");
    }
    Ok(())
}
