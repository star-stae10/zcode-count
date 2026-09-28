# zcode-count

监控**本机 ZCode 用量**的 Windows 桌面工具。它从 ZCode 的本地用量库增量读取 `model_usage` 记录，累积到自有库，并结合定价计算成本，在界面中展示汇总卡与三个页签（请求日志 / Provider 统计 / 模型统计）。

## 前置条件

- **必须安装 [cc-switch](https://github.com/farion1231/cc-switch) 以获取定价**：成本计算所需的模型单价来自 cc-switch 的本地数据库。未安装或缺少对应模型定价时，用量仍可统计，但成本会显示为「—」。
- **ZCode 需有本地用量库**：ZCode 使用过并产生用量记录后，其本地数据库才会存在。

## 数据源路径

| 用途 | 路径 |
| --- | --- |
| ZCode 用量库（只读） | `%USERPROFILE%\.zcode\cli\db\db.sqlite` |
| cc-switch 定价库（只读） | `%USERPROFILE%\.cc-switch\cc-switch.db` |
| 自有累积库（读写） | `%USERPROFILE%\.zcode-count\zcode-count.db` |

> 应用对 ZCode 与 cc-switch 的数据库**只读**访问；所有累积数据写入自有库。增量同步以 `db.sqlite`（含 `-wal`）的 mtime 作为水位线，重复同步幂等。

## 开发命令

```bash
pnpm install      # 安装依赖
pnpm tauri dev    # 启动开发环境（前端 Vite + Rust 后端）
pnpm test         # 运行前端单元测试（vitest）
```

## 构建

本地构建 Windows 安装包：

```bash
pnpm tauri build
```

产物位于：

- MSI：`src-tauri/target/release/bundle/msi/*.msi`
- NSIS 安装程序：`src-tauri/target/release/bundle/nsis/*-setup.exe`

## 下载安装包（推荐）

**从 GitHub Releases 下载**（无需登录 GitHub 账号）：

1. 打开仓库主页的 **Releases** 页（或直接访问 `https://github.com/star-stae10/zcode-count/releases/latest`）；
2. 在 **Assets** 中下载安装包，**优先选 NSIS 版** `zcode-count_<版本>_x64-setup.exe`（安装到用户目录，无需管理员权限）；MSI 版（`*.msi`）需要管理员权限安装；
3. 双击运行安装。首次安装如遇 Windows SmartScreen「已保护你的电脑」提示，属正常现象（应用未购买代码签名证书），点击「更多信息」→「仍要运行」即可。

每次推送 `v*` tag（如 `v1.0.0`）时，CI 会自动构建并创建对应的 GitHub Release。

> 更新方式：新版本发布后，下载新安装包直接覆盖安装即可（用户数据 `~/.zcode-count/zcode-count.db` 不受重装影响）。

## 从 GitHub Actions 下载（备用）

仓库包含 `build` 工作流（`.github/workflows/build.yml`），在以下情况触发：

- 手动触发（`workflow_dispatch`）
- 推送 `v*` 形式的 tag（如 `v1.0.0`）

工作流在 `windows-latest` 上运行 `pnpm install --frozen-lockfile`、`pnpm test`、`pnpm tauri build`，并把安装包上传为 artifact（tag 触发时同时创建 GitHub Release）。

artifact 下载方式（需登录 GitHub 账号）：

1. 打开仓库的 **Actions** 页，进入对应的 `build` 运行；
2. 在页面底部 **Artifacts** 区域下载 `zcode-count-windows`；
3. 解压后即可得到 `.msi` 与 `*-setup.exe` 安装包。
