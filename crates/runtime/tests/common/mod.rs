use rootcause::Report;
use zestors_runtime::prelude::*;

pub async fn simplest_handler(mut inbox: Inbox<()>) -> Result<(), Report> {
    while let Some(_) = inbox.next().await {}
    Ok(())
}
