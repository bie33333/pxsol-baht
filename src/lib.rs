#![allow(unexpected_cfgs)]

use solana_account_info::{next_account_info, AccountInfo};
use solana_address::Address;
use solana_cpi::invoke_signed;
use solana_msg::msg;
use solana_program_entrypoint::{entrypoint, ProgramResult};
use solana_program_error::ProgramError;
use solana_rent::Rent;
use solana_system_interface::instruction::create_account;
use solana_sysvar::Sysvar;

entrypoint!(process_instruction);

// ============================================================
// 常量
// ============================================================

const BALANCE_SIZE: usize = 8;

const MINTER_PUBKEY: [u8; 32] = [0u8; 32];

const OP_MINT: u8 = 0x00;
const OP_TRANSFER: u8 = 0x01;

// ============================================================
// 入口：解析 opcode 并路由
// ============================================================

pub fn process_instruction(
    program_id: &Address,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    if data.len() != 9 {
        msg!("Invalid instruction data length: {}", data.len());
        return Err(ProgramError::InvalidInstructionData);
    }

    let opcode = data[0];
    let amount = u64::from_be_bytes(data[1..9].try_into().unwrap());

    msg!("opcode: {}, amount: {}", opcode, amount);

    match opcode {
        OP_MINT => process_mint(program_id, accounts, amount),
        OP_TRANSFER => process_transfer(program_id, accounts, amount),
        _ => {
            msg!("Unknown opcode: {}", opcode);
            Err(ProgramError::InvalidInstructionData)
        }
    }
}

// ============================================================
// 铸造
// ============================================================

fn process_mint(
    program_id: &Address,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let account_minter = next_account_info(accounts_iter)?;
    let account_pda = next_account_info(accounts_iter)?;
    let _system_program = next_account_info(accounts_iter)?;
    let _rent_sysvar = next_account_info(accounts_iter)?;

    if !account_minter.is_signer {
        msg!("Minter is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    if account_minter.key.as_ref() != MINTER_PUBKEY {
        msg!("Only minter can mint");
        return Err(ProgramError::IllegalOwner);
    }

    let (expected_pda, _bump) = Address::find_program_address(
        &[account_minter.key.as_ref()],
        program_id,
    );
    if account_pda.key != &expected_pda {
        msg!("PDA mismatch");
        return Err(ProgramError::InvalidAccountData);
    }

    ensure_initialized(
        program_id,
        account_minter,
        account_minter,
        account_pda,
        accounts,
    )?;

    let balance = read_balance(account_pda)?;

    let new_balance = balance
        .checked_add(amount)
        .ok_or(ProgramError::InvalidInstructionData)?;

    write_balance(account_pda, new_balance);

    msg!("Minted {} baht, new balance: {}", amount, new_balance);
    Ok(())
}

// ============================================================
// 转账
// ============================================================

fn process_transfer(
    program_id: &Address,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let account_sender = next_account_info(accounts_iter)?;
    let account_sender_pda = next_account_info(accounts_iter)?;
    let account_receiver = next_account_info(accounts_iter)?;
    let account_receiver_pda = next_account_info(accounts_iter)?;
    let _system_program = next_account_info(accounts_iter)?;
    let _rent_sysvar = next_account_info(accounts_iter)?;

    if !account_sender.is_signer {
        msg!("Sender is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    let (expected_sender_pda, _) = Address::find_program_address(
        &[account_sender.key.as_ref()],
        program_id,
    );
    if account_sender_pda.key != &expected_sender_pda {
        msg!("Sender PDA mismatch");
        return Err(ProgramError::InvalidAccountData);
    }

    let (expected_receiver_pda, _) = Address::find_program_address(
        &[account_receiver.key.as_ref()],
        program_id,
    );
    if account_receiver_pda.key != &expected_receiver_pda {
        msg!("Receiver PDA mismatch");
        return Err(ProgramError::InvalidAccountData);
    }

    if account_sender_pda.lamports() == 0 {
        msg!("Sender PDA not initialized");
        return Err(ProgramError::UninitializedAccount);
    }

    ensure_initialized(
        program_id,
        account_sender,
        account_receiver,
        account_receiver_pda,
        accounts,
    )?;

    let sender_balance = read_balance(account_sender_pda)?;
    let receiver_balance = read_balance(account_receiver_pda)?;

    let new_sender = sender_balance
        .checked_sub(amount)
        .ok_or(ProgramError::InvalidInstructionData)?;
    let new_receiver = receiver_balance
        .checked_add(amount)
        .ok_or(ProgramError::InvalidInstructionData)?;

    write_balance(account_sender_pda, new_sender);
    write_balance(account_receiver_pda, new_receiver);

    msg!(
        "Transferred {} baht, sender: {} -> {}, receiver: {} -> {}",
        amount, sender_balance, new_sender, receiver_balance, new_receiver
    );
    Ok(())
}

// ============================================================
// 辅助函数
// ============================================================

fn ensure_initialized(
    program_id: &Address,
    payer: &AccountInfo,
    seeds_owner: &AccountInfo,
    pda: &AccountInfo,
    accounts: &[AccountInfo],
) -> ProgramResult {
    if pda.lamports() != 0 {
        if pda.data_len() != BALANCE_SIZE {
            msg!(
                "PDA data length mismatch: expected {}, got {}",
                BALANCE_SIZE,
                pda.data_len()
            );
            return Err(ProgramError::InvalidAccountData);
        }
        return Ok(());
    }

    msg!("Creating PDA: {}", pda.key);

    let rent_exemption = Rent::get()?.minimum_balance(BALANCE_SIZE);
    let (_, bump) = Address::find_program_address(
        &[seeds_owner.key.as_ref()],
        program_id,
    );

    let ix = create_account(
        payer.key,
        pda.key,
        rent_exemption,
        BALANCE_SIZE as u64,
        program_id,
    );

    invoke_signed(
        &ix,
        accounts,
        &[&[seeds_owner.key.as_ref(), &[bump]]],
    )?;

    msg!("PDA created, rent: {}", rent_exemption);
    Ok(())
}

fn read_balance(pda: &AccountInfo) -> Result<u64, ProgramError> {
    let data = pda.data.borrow();
    if data.len() < BALANCE_SIZE {
        return Err(ProgramError::InvalidAccountData);
    }
    let bytes: [u8; BALANCE_SIZE] = data[..BALANCE_SIZE].try_into().unwrap();
    Ok(u64::from_be_bytes(bytes))
}

fn write_balance(pda: &AccountInfo, balance: u64) {
    pda.data
        .borrow_mut()
        .copy_from_slice(&balance.to_be_bytes());
}