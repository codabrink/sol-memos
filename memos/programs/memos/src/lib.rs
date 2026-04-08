pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("DffF8iBBZwgq991CCkTcEAkhwBsvbihVd3Ch1r661C4A");

#[program]
pub mod memos {
    use super::*;
    pub fn store_memo(ctx: Context<StoreMemo>, text: String) -> Result<()> {
        store_memo::handler(ctx, text)
    }

}
