


# Someday
Create a `HandlePoolInit` trait like the following:
```rust
#[async_trait]
pub trait HandlePoolInit<I, T>: HandleProtocol {
    type PoolRef: Send + 'static;
    type PoolStartError: Send + 'static;

    async fn initialize(
        child: ChildPool<Self::Exit, <Self::State as HandlerState>::Inbox>,
        address: Address<<Self::State as HandlerState>::Inbox>,
    ) -> Result<
        (
            ChildPool<Self::Exit, <Self::State as HandlerState>::Inbox>,
            Self::PoolRef,
        ),
        Self::PoolStartError,
    >;

    async fn handle_init(init: I, item: T, state: &mut Self::State) -> Result<Self, Self::Exit>;
}
```

# Two problems:
1. `Action<H>` contaminates the whole protocol. Now a protocol becomes `MyProtocol<H>`, and subsequently the address becomes `Address<Inbox<MyPrototol<H>>>`.
2. `impl<T> HandleProtocol for T where T: HandleMyProtocol` does not work, since this is not allowed due to the trait system.