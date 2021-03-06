#[macro_use]
extern crate lazy_static;
extern crate bitintr;
extern crate numpy;
extern crate pyo3;
extern crate rand;
extern crate rayon;
extern crate serde;
extern crate serde_json;

pub mod bitboard;
pub mod checkmate;
pub mod mcts;
pub mod r#move;
pub mod neuralnetwork;
pub mod position;
pub mod record;
pub mod reservoir;
pub mod types;
pub mod zobrist;

use pyo3::prelude::*;

#[pymodule]
fn minishogilib(_py: Python, m: &PyModule) -> PyResult<()> {
    r#move::init();
    bitboard::init();
    zobrist::init();

    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    m.add_class::<position::Position>()?;
    m.add_class::<mcts::MCTS>()?;
    m.add_class::<r#move::Move>()?;

    m.add_class::<record::Record>()?;
    m.add_class::<reservoir::Reservoir>()?;

    Ok(())
}
