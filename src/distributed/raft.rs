use std::{sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard}, cmp, time::Duration};
use raft::{eraftpb, storage};

use crate::actor::Actor;

fn main() {
    let id = 0;
    let priority = 0;
    let election_tick = 10;

    let config = raft::Config {
        id,
        priority,
        election_tick,
        heartbeat_tick: 1,
        applied: 100,
        max_size_per_msg: u64::MAX,
        max_inflight_msgs: 500,
        check_quorum: true,
        pre_vote: true,
        min_election_tick: 0,
        max_election_tick: 0,
        read_only_option: raft::ReadOnlyOption::LeaseBased,
        skip_bcast_commit: false,
        batch_append: true,
        max_uncommitted_size: 100,
        max_committed_size_per_ready: 100,
    };

    config.validate().unwrap();

    let logger = raft::default_logger();

    let store = ZestorStore::default();

    let raft_node = raft::RawNode::new(&config, store, &logger);
}

//--------------------------------------------------------------------------------------------------
//  ZestorStore
//--------------------------------------------------------------------------------------------------

#[derive(Default)]
struct ZestorStore {
    core: Arc<RwLock<ZestorStoreCore>>,
}

impl ZestorStore {
    /// Aquire a read-lock for the `ZestorStoreCore`
    fn core_rl(&self) -> RwLockReadGuard<ZestorStoreCore> {
        self.core.read().unwrap()
    }

    /// Aquire a write-lock for the `ZestorStoreCore`
    fn core_wl(&self) -> RwLockWriteGuard<ZestorStoreCore> {
        self.core.write().unwrap()
    }
}

//--------------------------------------------------------------------------------------------------
//  ZestorsStoreCore
//--------------------------------------------------------------------------------------------------

struct ZestorStoreCore {
    raft_state: storage::RaftState,
    entries: Vec<eraftpb::Entry>,
    snapshot_metadata: eraftpb::SnapshotMetadata,
    trigger_snap_unavailable: bool,
}

impl Default for ZestorStoreCore {
    fn default() -> Self {
        Self {
            raft_state: Default::default(),
            entries: vec![],
            snapshot_metadata: Default::default(),
            trigger_snap_unavailable: false,
        }
    }
}

impl ZestorStoreCore {
    fn first_index(&self) -> u64 {
        match self.entries.first() {
            Some(e) => e.index,
            None => self.snapshot_metadata.index + 1,
        }
    }

    fn last_index(&self) -> u64 {
        match self.entries.last() {
            Some(e) => e.index,
            None => self.snapshot_metadata.index,
        }
    }

    fn snapshot(&self) -> eraftpb::Snapshot {
        let mut snapshot = eraftpb::Snapshot::default();

        // We assume all entries whose indexes are less than `hard_state.commit`
        // have been applied, so use the latest commit index to construct the snapshot.
        // TODO: This is not true for async ready.
        let meta = snapshot.mut_metadata();
        meta.index = self.raft_state.hard_state.commit;
        meta.term = match meta.index.cmp(&self.snapshot_metadata.index) {
            cmp::Ordering::Equal => self.snapshot_metadata.term,
            cmp::Ordering::Greater => {
                let offset = self.entries[0].index;
                self.entries[(meta.index - offset) as usize].term
            }
            cmp::Ordering::Less => {
                panic!(
                    "commit {} < snapshot_metadata.index {}",
                    meta.index, self.snapshot_metadata.index
                );
            }
        };

        meta.set_conf_state(self.raft_state.conf_state.clone());
        snapshot
    }
}

pub trait Test {
    fn test(&self);
}

//--------------------------------------------------------------------------------------------------
//  impl Storage for ZestorStore
//--------------------------------------------------------------------------------------------------

/// Storage saves all the information about the current Raft implementation, including Raft Log,
/// commit index, the leader to vote for, etc.
///
/// If any Storage method returns an error, the raft instance will
/// become inoperable and refuse to participate in elections; the
/// application is responsible for cleanup and recovery in this case.
impl raft::Storage for ZestorStore {
    /// `initial_state` is called when Raft is initialized. This interface will return a `RaftState`
    /// which contains `HardState` and `ConfState`.
    ///
    /// `RaftState` could be initialized or not. If it's initialized it means the `Storage` is
    /// created with a configuration, and its last index and term should be greater than 0.
    fn initial_state(&self) -> raft::Result<raft::RaftState> {
        Ok(self.core_rl().raft_state.clone())
    }

    /// Returns a slice of log entries in the range `[low, high)`.
    /// max_size limits the total size of the log entries returned if not `None`, however
    /// the slice of entries returned will always have length at least 1 if entries are
    /// found in the range.
    ///
    /// # Panics
    ///
    /// Panics if `high` is higher than `Storage::last_index(&self) + 1`.
    fn entries(
        &self,
        low: u64,
        high: u64,
        max_size: impl Into<Option<u64>>,
    ) -> raft::Result<Vec<raft::eraftpb::Entry>> {
        let max_size = max_size.into();
        let core = self.core_rl();
        if low < core.first_index() {
            return Err(raft::Error::Store(raft::StorageError::Compacted));
        }

        if high > core.last_index() + 1 {
            panic!(
                "index out of bound (last: {}, high: {})",
                core.last_index() + 1,
                high
            );
        }

        let offset = core.entries[0].index;
        let lo = (low - offset) as usize;
        let hi = (high - offset) as usize;
        let mut entries = core.entries[lo..hi].to_vec();
        raft::util::limit_size(&mut entries, max_size);
        Ok(entries)
    }

    /// Returns the term of entry idx, which must be in the range
    /// [first_index()-1, last_index()]. The term of the entry before
    /// first_index is retained for matching purpose even though the
    /// rest of that entry may not be available.
    fn term(&self, idx: u64) -> raft::Result<u64> {
        let core = self.core_rl();
        if idx == core.snapshot_metadata.index {
            return Ok(core.snapshot_metadata.term);
        }

        let offset = core.first_index();
        if idx < offset {
            return Err(raft::Error::Store(raft::StorageError::Compacted));
        }

        if idx > core.last_index() {
            return Err(raft::Error::Store(raft::StorageError::Unavailable));
        }
        Ok(core.entries[(idx - offset) as usize].term)
    }

    /// Returns the index of the first log entry that is possible available via entries, which will
    /// always equal to `truncated index` plus 1.
    ///
    /// New created (but not initialized) `Storage` can be considered as truncated at 0 so that 1
    /// will be returned in this case.
    fn first_index(&self) -> raft::Result<u64> {
        Ok(self.core_rl().first_index())
    }

    /// The index of the last entry replicated in the `Storage`.
    fn last_index(&self) -> raft::Result<u64> {
        Ok(self.core_rl().last_index())
    }

    /// Returns the most recent snapshot.
    ///
    /// If snapshot is temporarily unavailable, it should return SnapshotTemporarilyUnavailable,
    /// so raft state machine could know that Storage needs some time to prepare
    /// snapshot and call snapshot later.
    /// A snapshot's index must not less than the `request_index`.
    fn snapshot(&self, request_index: u64) -> raft::Result<raft::eraftpb::Snapshot> {
        let mut core = self.core_wl();

        if core.trigger_snap_unavailable {
            core.trigger_snap_unavailable = false;
            Err(raft::Error::Store(raft::StorageError::SnapshotTemporarilyUnavailable))
        } else {
            let mut snap = core.snapshot();
            if snap.get_metadata().index < request_index {
                snap.mut_metadata().index = request_index;
            }
            Ok(snap)
        }
    }
}
