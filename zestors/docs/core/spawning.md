# Spawn methods
A new actor can be spawned using one of the following methods:
- [`spawn`] : Spawn an actor consisting of one process that returns a [`Child`].
- [`spawn_one`] : Spawn an actor consisting of one process that returns a [`ChildGroup`].
- [`spawn_group`] : Spawn an actor consisting of multiple processes that returns a [`ChildGroup`].

Additional processes can be spawned onto an actor with:
- [`ChildGroup::spawn`]
- [`ChildGroup::try_spawn`]

# SpawnWith
Every spawn function takes an [`FnOnce`] to be spawned. One argument to this [`FnOnce`] is an [`S: Spawn`](Spawn). This argument can be one of:
- [`Halter`] : This actor will not be able to receive any messages. The config to spawn this is a [`Link`].
- [`Inbox<P> where P: Protocol`](Inbox) : This actor is able to receive messages according to the [`Protocol`]. The config to spawn this is a [`Config`].