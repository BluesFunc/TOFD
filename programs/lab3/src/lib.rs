use anchor_lang::prelude::*;

declare_id!("6f6B16pt9eT2WtTt6jBTEMucjCQyD5UyRMRf3T8aSUQb");

#[program]
pub mod lab3 {
    use super::*;

    pub fn initalize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Counter programm  ID {:?}", ctx.program_id);
        ctx.accounts.counter.value = 0;
        Ok(())
    }

    pub fn increment(ctx: Context<ChangeCounterValue>) -> Result<()>{

        let counter = &mut ctx.accounts.counter_account;
        counter.value += 1;
        msg!("Counter value increased. Current value equals {:?}", counter.value);
        Ok(())
    }

    pub fn decrement (ctx: Context<ChangeCounterValue>) -> Result<()> {
        
        let counter = &mut ctx.accounts.counter_account;
        counter.value -= 1;
        msg!("Counter value increased. Current value equals {:?}", counter.value);

        Ok(())
    }
}


#[derive(Accounts)]
pub struct Initialize<'info>{
    #[account(mut)]
    pub payer: Signer<'info>,
    
    #[account(
        init,
        payer = payer,
        space = 8 + 8
    )]
    pub counter: Account<'info, CounterAccount>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct ChangeCounterValue<'info> {
    #[account(mut)]
    pub counter_account: Account<'info, CounterAccount>,
}



#[account]
pub struct CounterAccount {
    value: u64
}