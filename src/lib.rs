#![feature(try_trait_v2)]

use std::{marker::PhantomData, mem::size_of_val};

use futures::Future;

pub mod actor;
pub mod address;
pub mod callable;
pub mod flow;
pub mod messaging;
pub mod packets;

macro_rules! func {
    ($x:expr) => {
        unsafe { Func::new(($x) as usize, $x) }
    };
}

fn main() {

    ().function(func!(test1));
    ().function(func!(test2));
}

fn test1(hi: u32) {}
fn test2(hi: u64) {}

trait Trait<F, V> {
    fn function(self, t: Func<F>);
}

impl<F: Fn(u32)> Trait<F, U32Func> for () {
    fn function(self, t: Func<F>) {}
}

impl<F: Fn(u64)> Trait<F, U64Func> for () {
    fn function(self, t: Func<F>) {}
}

struct Func<T> {
    ptr: usize,
    p: PhantomData<T>,
}

trait VarFunc<T> {}
struct U32Func;
struct U64Func;
impl<F: Fn(u32)> VarFunc<U32Func> for Func<F> {}
impl<F: Fn(u64)> VarFunc<U64Func> for Func<F> {}

impl<T> Func<T> {
    pub unsafe fn new(ptr: usize, f: T) -> Self {
        Self {
            ptr,
            p: PhantomData,
        }
    }
}
