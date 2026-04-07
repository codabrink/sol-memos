pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("9vGhzj9p9sZgMBkf7TSfyj9c9ruaADpBv2XWnTJqdKHM");

#[program]
pub mod memos {
    use super::*;
    pub fn store_memo(ctx: Context<StoreMemo>, text: String) -> Result<()> {
        store_memo::handler(ctx, text)
    }
}
