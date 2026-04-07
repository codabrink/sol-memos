use anchor_lang::prelude::*;

use crate::Memo;

pub const MAX_MEMO_SIZE: usize = 800;

pub fn handler(ctx: Context<StoreMemo>, text: String) -> Result<()> {
    ctx.accounts.memo.memo = text;
    Ok(())
}

#[derive(Accounts)]
#[instruction(text: String)]
pub struct StoreMemo<'info> {
    #[account(
        init,
        payer = signer,
        space = 8 + 4 + text.len(),
        constraint = text.len() <= MAX_MEMO_SIZE
    )]
    pub memo: Account<'info, Memo>,
    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
