use indexmap::IndexMap;
use reqwest::StatusCode;
use rootcause::report;
use zestors::{
    runtime::{ActorStatus, ChannelSnapshot, Name},
    supervision::ChildConfig,
};

pub struct Client {
    client: reqwest::Client,
    base_url: reqwest::Url,
}

impl Client {
    pub fn new(base_url: impl AsRef<str>) -> rootcause::Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            base_url: reqwest::Url::parse(base_url.as_ref())?,
        })
    }

    pub async fn get_processes(
        &self,
    ) -> rootcause::Result<IndexMap<Name, (ChildConfig, ActorStatus, Vec<Name>)>> {
        let url = self.base_url.join("/processes")?;
        let response = self.client.get(url).send().await?;

        match response.status() {
            StatusCode::OK => Ok(response.json().await?),
            _ => Err(response_error(response).await),
        }
    }

    pub async fn get_channel_snapshots(
        &self,
        names: Vec<Name>,
    ) -> rootcause::Result<Vec<Option<ChannelSnapshot>>> {
        let url = self.base_url.join("/snapshots")?;
        let response = self.client.get(url).json(&names).send().await?;

        match response.status() {
            StatusCode::OK => {
                let snapshots = response.json::<Vec<Option<ChannelSnapshot>>>().await?;
                Ok(snapshots)
            }
            _ => Err(response_error(response).await),
        }
    }
}

async fn response_error(response: reqwest::Response) -> rootcause::Report {
    let status = response.status();
    let error_text = response.text().await.unwrap_or_default();

    report!(
        "Unexpected status: {}. Response text: {}",
        status,
        error_text
    )
}
