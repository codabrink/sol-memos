use anchor_lang::prelude::*;

#[account]
pub struct Memo {
    pub memo: String,
    pub count: u64,
}

#[account]
pub struct MemoCounter {
    pub count: u64,
}
