
### Config
A [Config] decides how a process should be spawned, and is made up of two parts:
- The [Link] decides whether the process should be attached or detached, i.e. should it exit when the [Child] is dropped or not.
- The [Capacity] decides whether the [Inbox] should be bounded or unbounded.