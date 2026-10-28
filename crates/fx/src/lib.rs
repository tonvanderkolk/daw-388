//! Built-in effects of the 388's era: a plate reverb on AUX and a tape echo on EFF. Buffers are
//! allocated once, when an effect is made; `process` never allocates.

mod delay;
mod echo;
mod plate;

pub use echo::{Echo, EchoParams, EchoSync};
pub use plate::{Plate, PlateParams};
