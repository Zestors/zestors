use std::io::{Write, Read};

use crate::core::*;

use super::distr_addr::Distr;

//-------------------------------------------------
//  Future work
//-------------------------------------------------

impl<'a, M: 'a + Send> ParamType<'a, Distr> for Rcv<M> {
    type Param = Rcv<M>;
}

impl<'a, M: 'static + Send> DistrParamType<'a> for Rcv<M> {
    fn serialize_into(param: Self::Param, buf: impl Write) {
        unimplemented!()
    }

    fn deserialize_from(buf: impl Read) -> Self {
        unimplemented!()
    }

    fn serialized_size(param: &Self::Param) -> u64 {
        unimplemented!()
    }

    fn into_local_param(param: Self::Param) -> Self {
        param
    }
}

impl<'a, M: 'static + Send> ParamType<'a, Distr> for Snd<M> {
    type Param = Snd<M>;
}

impl<'a, M: 'static + Send> DistrParamType<'a> for Snd<M> {
    fn serialize_into(param: Self::Param, buf: impl Write) {
        unimplemented!()
    }

    fn deserialize_from(buf: impl Read) -> Self {
        unimplemented!()
    }

    fn serialized_size(param: &Self::Param) -> u64 {
        unimplemented!()
    }

    fn into_local_param(param: Self::Param) -> Self {
        param
    }
}
