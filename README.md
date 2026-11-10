# pxsol-baht

一个部署在 Solana 上的极简「泰铢币」程序。每个用户拥有一个由程序控制的 PDA 余额账户，支持铸造（仅限 Minter）和用户间转账。余额以固定 8 字节 `u64` 大端序存储在 PDA 的 `data` 中。

## 目录

- [功能概览](#功能概览)
- [目录结构](#目录结构)
- [前置要求](#前置要求)
- [快速开始](#快速开始)
  - [1. 准备钱包](#1-准备钱包)
  - [2. 部署合约](#2-部署合约)
  - [3. 配置前端](#3-配置前端)
  - [4. 运行前端](#4-运行前端)
- [测试流程](#测试流程)
- [常见问题](#常见问题)
- [架构说明](#架构说明)
- [扩展方向](#扩展方向)

---

## 功能概览

| 指令 | opcode | 说明 | 账户数 |
|---|---|---|---|
| 铸造 | `0x00` | 只有 `MINTER_PUBKEY` 对应钱包可调用，给自己的 PDA 增加余额 | 4 |
| 转账 | `0x01` | 任意已开户用户向另一个钱包的 PDA 转账 | 6 |

- **余额存储**：每个用户一个 PDA，`PDA = find_program_address([user.key], program_id)`，`data` 固定 8 字节，存 `u64` 大端余额。
- **自动开户**：铸造时自建自己的 PDA；转账时由发送方代付 rent，替接收方创建 PDA。
- **权限**：铸造权限硬编码在合约常量 `MINTER_PUBKEY` 里；转账只要求发送方签名，且双方 PDA 都经过派生校验。
- **原子性**：任何校验失败或余额溢出/不足，整笔交易回滚。

## 目录结构

```
pxsol-baht/
├── Cargo.toml          # 合约依赖
├── src/
│   └── lib.rs          # 合约源码
├── deploy.sh           # 构建 + 部署脚本
├── client/
│   └── index.html      # 前端页面（Phantom + web3.js）
└── README.md           # 本文件
```

## 前置要求

| 工具 | 版本 / 说明 | 安装 |
|---|---|---|
| Rust | stable | https://rustup.rs |
| Solana CLI | 2.x | https://docs.solana.com/cli/install-solana-cli-tools |
| `cargo-build-sbf` | 随 Solana CLI 安装 | `solana-install init` 后通常已带 |
| Python 3 | 用于 base58 解码 | 系统自带即可 |
| Phantom 钱包 | 浏览器扩展 | https://phantom.app |

验证安装：

```bash
rustc --version
solana --version
cargo-build-sbf --version
python3 --version
```

---

## 快速开始

### 1. 准备钱包

```bash
# 查看当前钱包（没有就创建）
solana address || solana-keygen new

# 切换到 devnet
solana config set --url https://api.devnet.solana.com

# 领测试 SOL
solana airdrop 2
solana balance
```

**记下这个钱包地址**，它既是你部署程序的付款方，也是默认的 **Minter**。

### 2. 部署合约

```bash
chmod +x deploy.sh
MINTER_ADDRESS=<你的钱包地址> ./deploy.sh
```

脚本会依次：

1. 用 Python 把 `MINTER_ADDRESS` 从 base58 解码成 `[u8; 32]`，写进 `src/lib.rs` 的 `MINTER_PUBKEY` 常量；
2. 执行 `cargo build-sbf`，生成 `./target/deploy/pxsol_baht.so`；
3. 首次运行会生成 `./target/deploy/pxsol_baht-keypair.json`（程序地址由它决定，**请勿删除**）；
4. 执行 `solana program deploy`，把程序部署到 devnet。

部署成功后，脚本末尾会打印：

```
✅ 部署完成
   Program ID:     7xKX...9fQ2
   Minter 地址:    <你的钱包地址>
```

**把 Program ID 复制下来**，下一步要用。

> 💡 想部署到 mainnet？把 `RPC_URL` 换掉即可：
> ```bash
> RPC_URL=https://api.mainnet-beta.solana.com MINTER_ADDRESS=... ./deploy.sh
> ```

### 3. 配置前端

打开 `client/index.html`，找到顶部两个常量，替换成你自己的值：

```js
const PROGRAM_ID     = new PublicKey("7xKX...9fQ2");  // ← deploy.sh 输出的 Program ID
const MINTER_ADDRESS = "YourWalletAddressHere...";    // ← 部署时用的钱包地址
```

同时确认 RPC 是你要的网络（默认 devnet）：

```js
const RPC = "https://api.devnet.solana.com";
```

### 4. 运行前端

从项目根目录启动静态服务器：

```bash
# 方式 A：Python
python3 -m http.server 8080
# 访问 http://localhost:8080/client/

# 方式 B：Node
npx serve .
# 访问 http://localhost:3000/client/
```

也可以进入 `client/` 目录单独启动：

```bash
cd client
python3 -m http.server 8080
# 访问 http://localhost:8080/
```

浏览器打开页面后，点击「连接 Phantom」。

> ⚠️ 不要直接双击 `index.html`。`<script type="module">` 在 `file://` 协议下会被部分浏览器拒绝加载，页面会空白。

---

## 测试流程

按下面的顺序逐步验证：

### 步骤 1 · 连接 Minter 钱包

- 用部署时那个钱包（`MINTER_ADDRESS`）连接 Phantom。
- 预期：
  - 钱包卡片「角色」显示 `Minter（可铸造）`。
  - 「铸造」按钮可点击。
  - PDA 地址已显示，余额 `0`，提示「PDA 未初始化」。

### 步骤 2 · 铸造 100 BAHT

- 「铸造」卡片输入 `100`，点击「铸造到我的 PDA」→ Phantom 签名 → 等待确认。
- 预期：
  - 余额变成 `100`。
  - 历史新增一条绿色「铸造」记录。
  - 在 [Solana Explorer](https://explorer.solana.com/?cluster=devnet) 搜索该签名，能看到 `process_mint` 的日志。

### 步骤 3 · 换普通钱包，验证权限

- 换一个非 Minter 的钱包重新连接。
- 预期：
  - 「角色」显示 `普通用户`。
  - 「铸造」按钮禁用。

### 步骤 4 · Minter 向普通钱包转账 30

- 换回 Minter 钱包。
- 「转账」卡片填入普通钱包地址和金额 `30` → 签名 → 确认。
- 预期：
  - Minter 余额变成 `70`。
  - 历史新增一条蓝色「转账」记录。

### 步骤 5 · 普通钱包确认收款

- 切换到普通钱包，点击「刷新余额」。
- 预期：
  - 余额显示 `30`。
  - `balanceMeta` 显示 `8 字节 · 890880 lamports`。

### 步骤 6 · 反向转账

- 普通钱包向 Minter 转 10。
- 预期：普通钱包余额 `20`，Minter 余额 `80`。

### 步骤 7 · 边界测试

| 测试 | 操作 | 预期 |
|---|---|---|
| 余额不足 | 转出大于当前余额 | 前端拦截「余额不足」，不发交易 |
| 自转账 | 接收者填自己地址 | 前端拦截「不能转给自己」 |
| 金额为 0 | 金额填 `0` | 前端拦截「金额必须大于 0」 |
| 金额非整数 | 金额填 `1.5` | 前端拦截「金额必须是整数」 |
| 未开户转账 | 绕过前端直接调合约 | 合约返回 `UninitializedAccount` |

### 步骤 8 · 查看链上状态（可选）

```bash
# 查程序部署信息
solana program show <你的 Program ID>

# 查某个用户的 PDA 账户
solana account <用户的 PDA 地址>
```

---

## 常见问题

### 铸造总是失败，报 `IllegalOwner`

**原因**：合约里的 `MINTER_PUBKEY` 和前端 `MINTER_ADDRESS` 不一致。

**排查**：

```bash
grep MINTER_PUBKEY src/lib.rs
grep MINTER_ADDRESS client/index.html
```

两个值解码后必须完全一致。如果合约里还是 `[0u8; 32]`，说明 `deploy.sh` 替换失败——可能 `MINTER_ADDRESS` 没传，或者 `src/lib.rs` 里格式不匹配。

**修复**：重新运行 `MINTER_ADDRESS=<地址> ./deploy.sh`，确认输出 `✅ 已替换 src/lib.rs 中的占位符`。

### 余额显示 18446744073709551615 之类的天文数字

**原因**：前后端字节序不一致。

**排查**：
- 合约：确认用的是 `to_be_bytes` / `from_be_bytes`；
- 前端：确认 `DataView.getBigUint64(0, false)` / `setBigUint64(1, ..., false)`，第三个参数 `false` 表示大端。

### 转账报 `UninitializedAccount`

**原因**：发送者的 PDA 不存在（从没铸造过，也没收到过钱）。

**修复**：先让 Minter 给你转一笔，或自己先铸造一次。

### 交易一直卡在「等待确认」

**原因**：RPC 节点拥堵或 blockhash 过期。

**修复**：
- 确认 `connection` 的 RPC 地址可访问；
- 检查钱包有没有 SOL（手续费不够交易会静默失败）；
- 重试，或把 `confirmTransaction` 的第二个参数从 `"confirmed"` 改成 `"finalized"`。

### `deploy.sh` 报 `cargo-build-sbf: command not found`

**原因**：Solana CLI 没装或没初始化。

**修复**：

```bash
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
solana-install init
# 重新打开终端后
cargo-build-sbf --version
```

### `solana program deploy` 报 `insufficient funds`

**原因**：钱包 SOL 不够付部署费（约 2–4 SOL）。

**修复**：

```bash
solana airdrop 5 --url https://api.devnet.solana.com
solana balance
```

### 换钱包后余额没更新

**原因**：余额是读链上快照，不会自动推送。

**修复**：点击「刷新余额」。切换 Phantom 账户后必须点「重新连接」，否则 `userPda` 还是旧账户的。

### 前端打开是空白页

**原因**：路径不对，或者用了 `file://` 协议。

**修复**：
- 确认访问的是 `http://localhost:8080/client/`（或 `client/` 目录下启动时是 `http://localhost:8080/`）；
- 不要直接双击 `index.html`。

---

## 架构说明

### 账户模型

```
用户钱包 ──派生──→ PDA
                    │
                    ├── owner = program_id
                    ├── data  = 8 字节（u64 大端余额）
                    └── lamports = 租金（约 890880）
```

- **一人一槽**：`seeds = [user.key]`，确保每个钱包对应唯一 PDA。
- **无全局账本**：余额分散在每个 PDA 里，不是合约里的一个 `mapping`。
- **owner = 程序**：只有本程序能改 PDA 的 `data` 和 `lamports`。

### 指令格式

```
data (9 字节)
┌─────────┬──────────────────────────┐
│ opcode  │ amount (u64, big-endian) │
│ 1 byte  │ 8 bytes                  │
└─────────┴──────────────────────────┘

opcode = 0x00 → 铸造
opcode = 0x01 → 转账
```

前端编码：`DataView.setBigUint64(1, amount, false)`。
合约解码：`u64::from_be_bytes(data[1..9])`。

### 账户顺序（ABI）

**铸造（4 个）**：

| 索引 | 账户 | isSigner | isWritable |
|---|---|---|---|
| 0 | minter 钱包 | ✅ | ✅ |
| 1 | minter PDA | ❌ | ✅ |
| 2 | System Program | ❌ | ❌ |
| 3 | Rent Sysvar | ❌ | ❌ |

**转账（6 个）**：

| 索引 | 账户 | isSigner | isWritable |
|---|---|---|---|
| 0 | sender 钱包 | ✅ | ✅ |
| 1 | sender PDA | ❌ | ✅ |
| 2 | receiver 钱包 | ❌ | ❌ |
| 3 | receiver PDA | ❌ | ✅ |
| 4 | System Program | ❌ | ❌ |
| 5 | Rent Sysvar | ❌ | ❌ |

**顺序即 ABI**。合约用 `next_account_info` 按位置取，前端 `keys` 数组必须严格对齐。

### 安全边界

| 检查 | 防住什么 |
|---|---|
| `is_signer` | 身份伪造：证明私钥持有者签了名 |
| `MINTER_PUBKEY` 比对 | 铸造权限：只有指定钱包能增发 |
| 双方 PDA 派生比对 | 身份与账户脱钩：防止用自己的签名写别人的槽 |
| `owner == program_id` | 越权修改：运行时强制，只能改本程序拥有的账户 |
| `checked_add` / `checked_sub` | 溢出/下溢：失败返回错误而非回绕 |

### 租金的特殊性

pxsol-ss 每次写入要按 `data.len()` 重算并补/退租金；pxsol-baht 的 PDA **大小固定 8 字节**，所以租金在创建时一次性付清，后续转账不再调整。

