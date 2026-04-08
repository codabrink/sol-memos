pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("ACG4xRMJg3gREHYKR3pNSdCvLidU8g39GEVbvGu43J2e");

#[program]
pub mod memos {
    use super::*;
    pub fn store_memo(ctx: Context<StoreMemo>, text: String) -> Result<()> {
        store_memo::handler(ctx, text)
    }
}
