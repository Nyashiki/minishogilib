//! NeuralNetworkに関係のある部分の実装
//!
//! ここでは、NeuralNetworkのForwardやBackpropagationなどを実装するのではなく、
//! tensorflow等の使用を容易にすることを目指す

#[cfg(test)]
use rand::seq::SliceRandom;

use position::Position;
use r#move::*;
use types::*;

use numpy::PyArray1;
use pyo3::prelude::*;

/// NeuralNetworkの入力層に与える形式に変換した際の、チャネル数
///
/// --------------------------------------------------------------
/// Feature                                             # Channels
/// --------------------------------------------------------------
/// P1 piece                                                    10
/// P2 piece                                                    10
/// Repetitions                                                  3
/// P1 prisoner count                                            5
/// P2 prisoner count                                            5
/// --------------------------------------------------------------
/// Color                                                        1
/// Total move count                                             1
/// --------------------------------------------------------------
/// Total                  (10 + 10 + 3 + 5 + 5) * HISTORY_NUM + 2
/// --------------------------------------------------------------

const HISTORY: usize = 8;
const CHANNEL_NUM_PER_HISTORY: usize = 10 + 10 + 3 + 5 + 5;
const CHANNEL_NUM: usize = CHANNEL_NUM_PER_HISTORY * HISTORY + 2;
const KP_INPUT_NUM: usize = (25 * 19 * 25) * 2 + 5 * 2 + 1 + 1 + 1;

impl Position {
    /// \[チャネル * y座標 * x座標\]の形式で返す
    pub fn to_alphazero_input_array(&self) -> [f32; CHANNEL_NUM * SQUARE_NB] {
        let mut input_layer = [0f32; CHANNEL_NUM * SQUARE_NB];

        let mut position = *self;

        for h in 0..HISTORY {
            if h > 0 {
                // 局面を1手戻す
                position.undo_move();
            }

            for i in 0..SQUARE_NB {
                // 盤上の駒を設定
                if position.board[i] != Piece::NO_PIECE {
                    if self.side_to_move == Color::WHITE {
                        input_layer[(2
                            + h * CHANNEL_NUM_PER_HISTORY
                            + piece_to_sequential_index(position.board[i]))
                            * SQUARE_NB
                            + i] = 1f32;
                    } else {
                        // 後手番の場合には、盤面を回転させて設定する
                        input_layer[(2
                            + h * CHANNEL_NUM_PER_HISTORY
                            + piece_to_sequential_index(position.board[i].get_op_piece()))
                            * SQUARE_NB
                            + (SQUARE_NB - i - 1)] = 1f32;
                    }
                }

                // 繰り返し回数を設定
                input_layer[(2 + h * CHANNEL_NUM_PER_HISTORY + 20 + position.get_repetition())
                    * SQUARE_NB
                    + i] = 1f32;
            }

            // 持ち駒を設定
            for piece_type in HAND_PIECE_TYPE_ALL.iter() {
                if position.hand[self.side_to_move.as_usize()][piece_type.as_usize() - 2] > 0 {
                    for i in 0..SQUARE_NB {
                        input_layer[(2
                            + h * CHANNEL_NUM_PER_HISTORY
                            + 23
                            + piece_type.as_usize()
                            - 2)
                            * SQUARE_NB
                            + i] = position.hand[self.side_to_move.as_usize()]
                            [piece_type.as_usize() - 2] as f32 / 2.0;
                    }
                }

                if position.hand[self.side_to_move.get_op_color().as_usize()]
                    [piece_type.as_usize() - 2]
                    > 0
                {
                    for i in 0..SQUARE_NB {
                        input_layer[(2
                            + h * CHANNEL_NUM_PER_HISTORY
                            + 28
                            + piece_type.as_usize()
                            - 2)
                            * SQUARE_NB
                            + i] = position.hand[self.side_to_move.get_op_color().as_usize()]
                            [piece_type.as_usize() - 2] as f32 / 2.0;
                    }
                }
            }

            if position.ply == 0 {
                break;
            }
        }

        // 手番を設定
        if self.side_to_move == Color::BLACK {
            for i in 0..SQUARE_NB {
                input_layer[i] = 1f32;
            }
        }

        // 手数を設定
        for i in 0..SQUARE_NB {
            input_layer[SQUARE_NB + i] = self.ply as f32 / MAX_PLY as f32;
        }

        return input_layer;
    }

    /// 11888要素のベクトルの形式で返す
    /// 25 * 19 * 25: 自分の玉の場所 * 自分の玉以外の駒の種類と場所
    /// 25 * 19 * 25: 相手の玉の場所 * 相手の玉以外の駒の種類と場所
    /// 5 * 2       : 持ち駒の数
    /// 1           : 手番
    /// 1           : 手数
    /// 1           : 繰り返し回数
    pub fn to_kp_input_array(&self) -> [f32; KP_INPUT_NUM] {
        let mut input_layer = [0f32; KP_INPUT_NUM];

        // 自分の玉に関するKP
        let my_king_square = if self.side_to_move == Color::WHITE {
            ::bitboard::get_square(self.piece_bb[Piece::W_KING.as_usize()])
        } else {
            ::bitboard::get_square(self.piece_bb[Piece::B_KING.as_usize()])
        };

        let offset = if self.side_to_move == Color::WHITE {
            my_king_square * 19 * 25
        } else {
            (SQUARE_NB - 1 - my_king_square) * 19 * 25
        };

        for i in 0..SQUARE_NB {
            if i == my_king_square || self.board[i] == Piece::NO_PIECE {
                continue;
            }

            if self.side_to_move == Color::WHITE {
                let index = (piece_to_sequential_index(self.board[i]) - 1) * 25 + i;
                input_layer[offset + index] = 1.0;
            } else {
                let index = (piece_to_sequential_index(self.board[i].get_op_piece()) - 1) * 25
                    + (SQUARE_NB - 1 - i);
                input_layer[offset + index] = 1.0;
            }
        }

        // 相手の玉に関するKP
        let op_king_square = if self.side_to_move == Color::WHITE {
            ::bitboard::get_square(self.piece_bb[Piece::B_KING.as_usize()])
        } else {
            ::bitboard::get_square(self.piece_bb[Piece::W_KING.as_usize()])
        };

        let offset = if self.side_to_move == Color::WHITE {
            25 * 19 * 25 + op_king_square * 19 * 25
        } else {
            25 * 19 * 25 + (SQUARE_NB - 1 - op_king_square) * 19 * 25
        };

        for i in 0..SQUARE_NB {
            if i == op_king_square || self.board[i] == Piece::NO_PIECE {
                continue;
            }

            if self.side_to_move == Color::WHITE {
                let index = if (self.board[i].as_usize()) < (Piece::B_KING.as_usize()) {
                    piece_to_sequential_index(self.board[i]) * 25 + i
                } else {
                    (piece_to_sequential_index(self.board[i]) - 1) * 25 + i
                };
                input_layer[offset + index] = 1.0;
            } else {
                let index = if (self.board[i].as_usize()) < (Piece::B_KING.as_usize()) {
                    piece_to_sequential_index(self.board[i]) * 25 + (SQUARE_NB - 1 - i)
                } else {
                    (piece_to_sequential_index(self.board[i]) - 1) * 25 + (SQUARE_NB - 1 - i)
                };
                input_layer[offset + index] = 1.0;
            }
        }

        for piece_type in HAND_PIECE_TYPE_ALL.iter() {
            input_layer[25 * 19 * 25 * 2 + piece_type.as_usize() - 2] =
                self.hand[self.side_to_move.as_usize()][piece_type.as_usize() - 2] as f32;
            input_layer[25 * 19 * 25 * 2 + 5 + piece_type.as_usize() - 2] = self.hand
                [self.side_to_move.get_op_color().as_usize()][piece_type.as_usize() - 2]
                as f32;
        }

        if self.side_to_move == Color::BLACK {
            input_layer[25 * 19 * 25 * 2 + 5 * 2] = 1.0;
        }

        input_layer[25 * 19 * 25 * 2 + 5 * 2 + 1] = self.ply as f32;
        input_layer[25 * 19 * 25 * 2 + 5 * 2 + 2] = self.get_repetition() as f32;

        return input_layer;
    }
}

#[pymethods]
impl Position {
    pub fn to_alphazero_input(&self, py: Python) -> Py<PyArray1<f32>> {
        return PyArray1::from_slice(py, &self.to_alphazero_input_array()).to_owned();
    }

    pub fn to_kp_input(&self, py: Python) -> Py<PyArray1<f32>> {
        return PyArray1::from_slice(py, &self.to_kp_input_array()).to_owned();
    }
}

#[pymethods]
impl Move {
    pub fn to_policy_index(&self) -> usize {
        let c: Color = self.piece.get_color();

        let index = if self.is_hand {
            if c == Color::WHITE {
                (64 + self.get_hand_index(), self.to)
            } else {
                (64 + self.get_hand_index(), SQUARE_NB - 1 - self.to)
            }
        } else {
            let (direction, amount) = get_relation(self.from, self.to);
            assert!(amount > 0);

            if self.get_promotion() {
                if c == Color::WHITE {
                    (32 + 4 * direction as usize + amount - 1, self.from)
                } else {
                    (
                        32 + 4 * ((direction as usize + 4) % 8) + amount - 1,
                        SQUARE_NB - 1 - self.from,
                    )
                }
            } else {
                if c == Color::WHITE {
                    (4 * direction as usize + amount - 1, self.from)
                } else {
                    (
                        4 * ((direction as usize + 4) % 8) + amount - 1,
                        SQUARE_NB - 1 - self.from,
                    )
                }
            }
        };

        return index.0 * SQUARE_NB + index.1;
    }
}

#[cfg(test)]
fn index_to_move(position: &Position, index: usize) -> Move {
    let mut moves: std::vec::Vec<Move> = Vec::new();

    if index >= 64 * 25 {
        for i in 0..5 {
            for j in 0..SQUARE_NB {
                let temp = if position.side_to_move == Color::WHITE {
                    (64 + i) * 25 + j
                } else {
                    (64 + i) * 25 + (SQUARE_NB - j - 1)
                };

                if temp == index {
                    moves.push(Move::hand_move(
                        HAND_PIECE_TYPE_ALL[i].get_piece(position.side_to_move),
                        j,
                    ));
                }
            }
        }
    } else {
        for direction in 0..8 {
            for amount in 0..4 {
                for i in 0..SQUARE_NB {
                    for promotion in 0..2 {
                        let temp = if position.side_to_move == Color::WHITE {
                            (32 * promotion + ((direction * 4) + amount)) * 25 + i
                        } else {
                            (32 * promotion + ((((direction + 4) % 8) * 4) + amount)) * 25
                                + (SQUARE_NB - i - 1)
                        };

                        if temp == index {
                            moves.push(Move::board_move(
                                Piece::NO_PIECE,
                                i,
                                0,
                                promotion != 0,
                                Piece::NO_PIECE,
                            ));
                        }
                    }
                }
            }
        }
    }

    assert_eq!(moves.len(), 1);
    return moves[0];
}

#[test]
fn to_policy_index_test() {
    ::bitboard::init();

    const LOOP_NUM: i32 = 10000;

    let mut position = Position::empty_board();

    let mut rng = rand::thread_rng();

    for _ in 0..LOOP_NUM {
        position.set_start_position();

        while position.ply < MAX_PLY as u16 {
            let moves = position.generate_moves();

            for m in &moves {
                let index = m.to_policy_index();
                let move_from_index = index_to_move(&position, index);

                if m.is_hand {
                    assert_eq!(m.to, move_from_index.to);
                } else {
                    assert_eq!(m.from, move_from_index.from);
                    assert_eq!(m.promotion, move_from_index.promotion);
                }
            }

            // ランダムに局面を進める
            if moves.len() == 0 {
                break;
            }

            let random_move = moves.choose(&mut rng).unwrap();
            position.do_move(random_move);
        }
    }
}

fn piece_to_sequential_index(piece: Piece) -> usize {
    if piece.get_color() == Color::WHITE {
        if piece.is_raw() {
            piece.as_usize() - 1
        } else {
            piece.as_usize() - 5
        }
    } else {
        if piece.is_raw() {
            piece.as_usize() - 7
        } else {
            piece.as_usize() - 11
        }
    }
}

#[test]
fn piece_to_sequential_index_test() {
    assert_eq!(piece_to_sequential_index(Piece::W_KING), 0);
    assert_eq!(piece_to_sequential_index(Piece::W_GOLD), 1);
    assert_eq!(piece_to_sequential_index(Piece::W_SILVER), 2);
    assert_eq!(piece_to_sequential_index(Piece::W_BISHOP), 3);
    assert_eq!(piece_to_sequential_index(Piece::W_ROOK), 4);
    assert_eq!(piece_to_sequential_index(Piece::W_PAWN), 5);
    assert_eq!(piece_to_sequential_index(Piece::W_SILVER_X), 6);
    assert_eq!(piece_to_sequential_index(Piece::W_BISHOP_X), 7);
    assert_eq!(piece_to_sequential_index(Piece::W_ROOK_X), 8);
    assert_eq!(piece_to_sequential_index(Piece::W_PAWN_X), 9);

    assert_eq!(piece_to_sequential_index(Piece::B_KING), 10);
    assert_eq!(piece_to_sequential_index(Piece::B_GOLD), 11);
    assert_eq!(piece_to_sequential_index(Piece::B_SILVER), 12);
    assert_eq!(piece_to_sequential_index(Piece::B_BISHOP), 13);
    assert_eq!(piece_to_sequential_index(Piece::B_ROOK), 14);
    assert_eq!(piece_to_sequential_index(Piece::B_PAWN), 15);
    assert_eq!(piece_to_sequential_index(Piece::B_SILVER_X), 16);
    assert_eq!(piece_to_sequential_index(Piece::B_BISHOP_X), 17);
    assert_eq!(piece_to_sequential_index(Piece::B_ROOK_X), 18);
    assert_eq!(piece_to_sequential_index(Piece::B_PAWN_X), 19);
}
