
Supervising an actor can be done by waiting for it to exit using it's [Child]. This returns the exit-values of the processes if the actor exits exits normally, otherwise it returns an [ExitError]. An actor can be stopped by either halting or aborting it.

# Child
A [`Child`] is a unique reference to an actor, similar to a [tokio JoinHandle](tokio::task::JoinHandle), but different in that it could refer to multiple processes instead of just one. By default, when a child is dropped the processes that the child referred to are shut down; this allows for the creation of supervision trees.

If an actor contains multiple processes, then instead of a [`Child`] it is refered to as a [`ChildGroup`]. A childgroup can't be awaited, but instead the childgroup can be [streamed](futures::Stream) to collect the exit-values of the processes.

# Stopping

### Halt
An actor can be halted using its [Child] or [Address]. Once the actor is aborted, all processes should clean up their state and then exit gracefully. When the actor is halted, its [Inbox] is also automatically closed, and no more messages can be sent to the inbox.

### Abort
An actor can be aborted using [Child::abort]. Aborting will forcefully interrupt the process at its first `.await` point, and not allow it to clean up it's state before exiting. ([see tokio abort](tokio::task::JoinHandle::abort))

### Shutdown
An actor can be shut down using its [Child]. This will first attempt to halt the actor until a certain amout of time has passed, if the actor has not exited by that point it is aborted

# Actor state
The state of an actor can be queried from its [Child] or [Address] with four different methods:
- `is_finished`: Returns true if all underlying tasks have finished. (Only for children)
- `has_exited`: Returns true if all [Inboxes](Inbox) have been dropped.
- `is_aborted`: Returns true if the actor has been aborted. (Only for children)
- `is_closed`: Returns true if the [Inbox] has been closed.


