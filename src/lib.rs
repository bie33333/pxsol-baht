#![allow(unexpected_cfgs)]

use solana_account_info::{next_account_info, AccountInfo};
use solana_cpi::invoke_signed;
use solana_msg::msg;
use solana_program_entrypoint::{entrypoint, ProgramResult};
use solana_program_error::ProgramError;
use solana_pubkey::Pubkey;
use solana_rent::Rent;
use solana_system_interface::instruction::create_account;
use solana_sysvar::Sysvar;

entrypoint!(process_instruction);

// ============================================================
// 常量
// ============================================================

/// 余额账户固定大小：8 字节 u64 大端序
const BALANCE_SIZE: usize = 8;

/// 铸造权限：只有这个公钥可以发起铸造
///
/// ⚠️ 这是占位值（全 0），实际部署时必须替换！
/// 替换方法：
/// 1. 运行 `solana address` 或从 Phantom 复制 Minter 钱包地址（base58）
/// 2. 用 bs58 解码为 32 字节
/// 3. 替换下方数组
/// 4. 或者直接用 deploy.sh 的 MINTER_ADDRESS 参数自动替换
const MINTER_PUBKEY: [u8; 32] = [0u8; 32];

/// 指令 opcode
const OP_MINT: u8 = 0x00;
const OP_TRANSFER: u8 = 0x01;

// ============================================================
// 入口：解析 opcode 并路由
// ============================================================

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    // ============ 1. 解析指令 ============
    // data 格式：[opcode: 1 字节][amount: 8 字节 u64 大端]
    if data.len() != 9 {
        msg!("Invalid instruction data length: {}", data.len());
        return Err(ProgramError::InvalidInstructionData);
    }

    let opcode = data[0];
    let amount = u64::from_be_bytes(data[1..9].try_into().unwrap());

    msg!("opcode: {}, amount: {}", opcode, amount);

    // ============ 2. 路由 ============
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
//
// 账户（顺序即 ABI）：
// [0] minter wallet  (signer, writable)  —— 必须是 MINTER_PUBKEY
// [1] minter PDA     (writable)          —— 该钱包的余额账户
// [2] system program (readonly)
// [3] rent sysvar    (readonly)
//
// 流程：
// 1. 校验签名 + 校验是 minter
// 2. 校验 PDA 派生正确
// 3. 若 PDA 未初始化 → create_account
// 4. 读余额 → checked_add(amount) → 写回

fn process_mint(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let account_minter = next_account_info(accounts_iter)?;
    let account_pda = next_account_info(accounts_iter)?;
    let _system_program = next_account_info(accounts_iter)?;
    let _rent_sysvar = next_account_info(accounts_iter)?;

    // 1. 签名校验
    if !account_minter.is_signer {
        msg!("Minter is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // 2. 权限校验：只有 minter 能铸造
    if account_minter.key.to_bytes() != MINTER_PUBKEY {
        msg!("Only minter can mint");
        return Err(ProgramError::IllegalOwner);
    }

    // 3. PDA 校验
    let (expected_pda, _bump) = Pubkey::find_program_address(
        &[&account_minter.key.to_bytes()],
        program_id,
    );
    if account_pda.key != &expected_pda {
        msg!("PDA mismatch");
        return Err(ProgramError::InvalidAccountData);
    }

    // 4. 确保 PDA 已初始化
    ensure_initialized(
        program_id,
        account_minter, // 付款方
        account_minter, // seeds 来源
        account_pda,
        accounts,
    )?;

    // 5. 读余额
    let balance = read_balance(account_pda)?;

    // 6. 计算新余额
    let new_balance = balance
        .checked_add(amount)
        .ok_or(ProgramError::InvalidInstructionData)?;

    // 7. 写回
    write_balance(account_pda, new_balance);

    msg!("Minted {} baht, new balance: {}", amount, new_balance);
    Ok(())
}

// ============================================================
// 转账
// ============================================================
//
// 账户（顺序即 ABI）：
// [0] sender wallet   (signer, writable)
// [1] sender PDA      (writable)
// [2] receiver wallet (readonly)
// [3] receiver PDA    (writable)
// [4] system program  (readonly)
// [5] rent sysvar     (readonly)
//
// 流程：
// 1. 校验签名 + 双方 PDA 派生正确
// 2. 发送者 PDA 必须已存在（lamports > 0）
// 3. 接收者 PDA 若未初始化 → create_account（付款方是发送者）
// 4. 读双方余额 → checked_sub / checked_add → 写回

fn process_transfer(
    program_id: &Pubkey,
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

    // 1. 签名校验
    if !account_sender.is_signer {
        msg!("Sender is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // 2. 发送者 PDA 校验
    let (expected_sender_pda, _) = Pubkey::find_program_address(
        &[&account_sender.key.to_bytes()],
        program_id,
    );
    if account_sender_pda.key != &expected_sender_pda {
        msg!("Sender PDA mismatch");
        return Err(ProgramError::InvalidAccountData);
    }

    // 3. 接收者 PDA 校验
    let (expected_receiver_pda, _) = Pubkey::find_program_address(
        &[&account_receiver.key.to_bytes()],
        program_id,
    );
    if account_receiver_pda.key != &expected_receiver_pda {
        msg!("Receiver PDA mismatch");
        return Err(ProgramError::InvalidAccountData);
    }

    // 4. 发送者 PDA 必须已存在
    if account_sender_pda.lamports() == 0 {
        msg!("Sender PDA not initialized");
        return Err(ProgramError::UninitializedAccount);
    }

    // 5. 确保接收者 PDA 已初始化
    ensure_initialized(
        program_id,
        account_sender,   // 付款方：发送者
        account_receiver, // seeds 来源：接收者
        account_receiver_pda,
        accounts,
    )?;

    // 6. 读双方余额
    let sender_balance = read_balance(account_sender_pda)?;
    let receiver_balance = read_balance(account_receiver_pda)?;

    // 7. 计算新余额
    let new_sender = sender_balance
        .checked_sub(amount)
        .ok_or(ProgramError::InvalidInstructionData)?;
    let new_receiver = receiver_balance
        .checked_add(amount)
        .ok_or(ProgramError::InvalidInstructionData)?;

    // 8. 写回
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

/// 确保 PDA 已初始化
///
/// - `payer`：支付 rent 的钱包（必须有签名/可写）
/// - `seeds_owner`：派生 PDA 的钱包（决定 seeds 和 bump）
/// - `pda`：目标 PDA
fn ensure_initialized(
    program_id: &Pubkey,
    payer: &AccountInfo,
    seeds_owner: &AccountInfo,
    pda: &AccountInfo,
    accounts: &[AccountInfo],
) -> ProgramResult {
    if pda.lamports() != 0 {
        // 已初始化：只检查大小
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

    // 未初始化：创建
    msg!("Creating PDA: {}", pda.key);

    let rent_exemption = Rent::get()?.minimum_balance(BALANCE_SIZE);
    let (_, bump) = Pubkey::find_program_address(
        &[&seeds_owner.key.to_bytes()],
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
        &[&[&seeds_owner.key.to_bytes(), &[bump]]],
    )?;

    msg!("PDA created, rent: {}", rent_exemption);
    Ok(())
}

/// 读取余额（8 字节 u64 大端序）
fn read_balance(pda: &AccountInfo) -> Result<u64, ProgramError> {
    let data = pda.data.borrow();
    if data.len() < BALANCE_SIZE {
        return Err(ProgramError::InvalidAccountData);
    }
    let bytes: [u8; BALANCE_SIZE] = data[..BALANCE_SIZE].try_into().unwrap();
    Ok(u64::from_be_bytes(bytes))
}

/// 写入余额（8 字节 u64 大端序）
fn write_balance(pda: &AccountInfo, balance: u64) {
    pda.data
        .borrow_mut()
        .copy_from_slice(&balance.to_be_bytes());
}