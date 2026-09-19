#![allow(dead_code)]

use zestors_codegen::Message;

fn main() {}

#[derive(Message)]
#[zestors(interface_path = "zestors_interface")]
struct MyMessage;
