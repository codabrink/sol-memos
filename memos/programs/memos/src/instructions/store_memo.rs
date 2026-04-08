use anchor_lang::prelude::*;

use crate::{Memo, MemoCounter};

pub const MAX_MEMO_SIZE: usize = 800;

pub fn handler(ctx: Context<StoreMemo>, text: String) -> Result<()> {
    ctx.accounts.memo.memo = text;
    ctx.accounts.memo.count = ctx.accounts.counter.count;
    ctx.accounts.counter.count += 1;
    Ok(())
}

#[derive(Accounts)]
#[instruction(text: String)]
pub struct StoreMemo<'info> {
    #[account(
        init_if_needed,
        payer = signer,
        space = 8 + 8,
        seeds = [signer.key().as_ref(), b"counter"],
        bump,
    )]
    pub counter: Account<'info, MemoCounter>,
    #[account(
        init,
        payer = signer,
        space = 8 + 4 + text.len() + 8,
        constraint = text.len() <= MAX_MEMO_SIZE,
        seeds = [signer.key().as_ref(), b"memo", &counter.count.to_le_bytes()],
        bump,
    )]
    pub memo: Account<'info, Memo>,
    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
