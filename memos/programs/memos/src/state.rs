use anchor_lang::prelude::*;

#[account]
pub struct Memo {
    pub memo: String,

    // A copy of count is kept here to be used for ordering in the client.
    // If we don't care about memo order, then this can be discarded.
    pub count: u64,
}

#[account]
pub struct MemoCounter {
    pub count: u64,
}
