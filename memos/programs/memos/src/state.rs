use anchor_lang::prelude::*;

#[account]
pub struct Memo {
    pub memo: String,
}
