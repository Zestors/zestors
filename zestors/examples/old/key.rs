// use zestors::supervision::Key;

// fn main() {}

// // #[key]
// // #[repr(u32)]
// // enum MyKey {
// //     Test(u32) = 1,
// //     A = 2,
// //     B = 4,
// // }

// #[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
// #[repr(u32)]
// enum MyKey {
//     One(u32) = 0,
//     Test(u32) = 1,
//     A = 2,
//     B = 4,
// }

// use zestors::supervision_v2::DynamicChildSpec;

fn main() {}

// // Generates this impl
// impl From<MyKey> for Key {
//     fn from(t: MyKey) -> Key {
//         fn _zestors_priv_combine(a: u32, b: u32) -> Box<[u8; 8]> {
//             Box::new((((a as u64) << 32) | (b as u64)).to_ne_bytes())
//         }
//         fn _zestors_priv_single(a: u32) -> Box<[u8; 4]> {
//             Box::new(a.to_ne_bytes())
//         }
//         match t {
//             MyKey::One(val) => Key::new(_zestors_priv_combine(0, val)),
//             MyKey::Test(val) => Key::new(_zestors_priv_combine(1, val)),
//             MyKey::A => Key::new(_zestors_priv_single(2)),
//             MyKey::B => Key::new(_zestors_priv_single(4)),
//         }
//     }
// }

// impl TryFrom<Key> for MyKey {
//     type Error = ();

//     fn try_from(value: Key) -> Result<Self, Self::Error> {
//         if value.inner().len() < 4 {
//             return Err(());
//         }

//         let (a_slice, b_slice) = value.inner().split_at(4);

//         let a = u32::from_ne_bytes(a_slice.try_into().unwrap());
//         let b = match b_slice.try_into() {
//             Ok(b) => {
//                 if b_slice.len() != 4 {
//                     return Err(());
//                 }
//                 Some(u32::from_ne_bytes(b))
//             }
//             Err(_) => {
//                 if b_slice.len() != 0 {
//                     return Err(());
//                 }
//                 None
//             }
//         };

//         match (a, b) {
//             (0, Some(b)) => Ok(Self::One(b)),
//             (1, Some(b)) => Ok(Self::Test(b)),
//             (2, None) => Ok(Self::A),
//             (4, None) => Ok(Self::B),
//             _ => Err(()),
//         }
//     }
// }
