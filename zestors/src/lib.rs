#![doc = include_str!("../docs/lib.md")]

mod helper;

pub mod supervision {
    #![doc = include_str!("../docs/supervision.md")]
    #[allow(unused_imports)]
    use crate::helper::inline_docs;
    inline_docs!(
        pub use zestors_supervision::*;
    );
}

pub mod distribution {
    #![doc = include_str!("../docs/distribution.md")]
    #[allow(unused_imports)]
    use crate::helper::inline_docs;
    inline_docs!(
        pub use zestors_distribution::*;
    );
}

pub mod core {
    #![doc = include_str!("../docs/core/mod.md")]
    use crate::helper::inline_docs;

    pub mod protocol {
        #![doc = include_str!("../docs/core/protocol.md")]
        #[allow(unused_imports)]
        use super::*;
        inline_docs!(
            pub use zestors_codegen::{protocol, Message};
            pub use zestors_core::protocol::*;
            pub use zestors_request::*;
        );
    }

    pub mod channel {
        #![doc = include_str!("../docs/core/channel.md")]
        #[allow(unused_imports)]
        use super::*;
        inline_docs!(
            pub use zestors_core::channel::*;
            pub use zestors_channels::halter_channel::*;
            pub use zestors_channels::inbox_channel::*;
        );
    }

    pub mod config {
        #![doc = include_str!("../docs/core/config.md")]
        #[allow(unused_imports)]
        use super::*;
        inline_docs!(
            pub use zestors_core::config::*;
        );
    }

    pub mod sending {
        #![doc = include_str!("../docs/core/sending.md")]
        #[allow(unused_imports)]
        use super::*;
        inline_docs!(
            pub use zestors_core::sending::*;
        );
    }

    pub mod receiving {
        #![doc = include_str!("../docs/core/receiving.md")]
        #[allow(unused_imports)]
        use super::*;
        inline_docs!(
            pub use zestors_channels::receiving::*;
            pub use zestors_channels::halter::*;
        );
    }

    pub mod spawning {
        #![doc = include_str!("../docs/core/spawning.md")]
        #[allow(unused_imports)]
        use super::*;
        inline_docs!(
            pub use zestors_core::spawning::*;
        );
    }

    pub mod supervision {
        #![doc = include_str!("../docs/core/supervision.md")]
        #[allow(unused_imports)]
        use super::*;
        inline_docs!(
            pub use zestors_core::supervision::*;
        );
    }

    inline_docs!(
        pub use protocol::*;
        pub use channel::*;
        pub use config::*;
        pub use sending::*;
        pub use spawning::*;
        pub use receiving::*;
        pub use supervision::*;
    );
}
