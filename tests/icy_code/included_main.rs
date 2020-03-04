#![feature(const_trait_impl, const_fn)]

pub trait MyTrait { fn method(&self); }

impl const MyTrait for std::convert::Infallible {
    #[inline(always)]
    fn method(&self) { match *self {} }
}

fn main() { }
