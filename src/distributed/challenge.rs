use serde::{Deserialize, Serialize};
use sha2::{digest::Digest, Sha256};
use uuid::Uuid;
// use sha2::Digest;
use super::local_node::{self, LocalNode};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) struct Challenge(Uuid);

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub(crate) struct ChallengeResponse(Box<[u8]>, Box<[u8]>);

impl ChallengeResponse {
    pub(crate) fn new(local_node: &LocalNode, challenge: &Challenge) -> Self {
        let mut build_hash = Sha256::new();
        build_hash.update(challenge.0.as_bytes());
        build_hash.update(local_node.build_id().as_bytes());
        let build_hash: Box<[u8]> = build_hash.finalize().as_slice().into();

        let mut cookie_hash = Sha256::new();
        cookie_hash.update(challenge.0.as_bytes());
        cookie_hash.update(local_node.cookie().as_bytes());
        let cookie_hash: Box<[u8]> = cookie_hash.finalize().as_slice().into();

        Self(build_hash, cookie_hash)
    }

    pub(crate) fn check_challenge(
        &self,
        challenge: &Challenge,
        local_node: &LocalNode,
    ) -> Result<(), ChallengeError> {
        let actual_response = ChallengeResponse::new(local_node, challenge);

        match (self.0 == actual_response.0, self.1 == actual_response.1) {
            (true, true) => Ok(()),
            (true, false) => Err(ChallengeError::CookieDiffers),
            (false, true) => Err(ChallengeError::BuildDiffers),
            (false, false) => Err(ChallengeError::CookieAndBuildDiffer),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ChallengeError {
    CookieDiffers,
    BuildDiffers,
    CookieAndBuildDiffer,
}

impl Challenge {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }
}
