# DC License Bot

[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?style=for-the-badge&logo=discord&logoColor=white)](https://discord.com/)

一个用 Rust 构建的 Discord 机器人，专门用于创作者作品的许可协议管理。机器人提供完整的许可协议声明、管理和自动发布功能，遵循**反商业化**原则。

## ✨ 核心功能

### 📝 许可协议管理
- **创建自定义协议** - 用户可创建个性化的许可协议（限制5个）
- **协议管理面板** - 查看、编辑、删除已创建的协议
- **智能协议发布** - 在 Discord 帖子中应用许可协议
- **权限验证** - 确保只有作品作者可以添加协议

### ⚡ 自动化功能
- **自动发布设置** - 在指定论坛频道发帖时自动附加许可协议
- **默认协议配置** - 设置常用的默认许可协议
- **协议更新替换** - 自动废弃旧协议并发布新版本
- **备份权限通知** - 集成外部备份服务，权限变更时自动通知

### 🛡️ 管理员功能
- **系统信息监控** - 查看机器人运行状态和性能指标
- **热重载系统授权** - 无需重启即可更新系统许可配置
- **权限管理** - 基于配置文件的灵活权限控制

## 🏗️ 技术架构

### 分层架构设计
```
┌─────────────────────────────────────────┐
│            Commands Layer               │  ← Poise 斜杠命令框架
├─────────────────────────────────────────┤
│            Handlers Layer               │  ← Discord 事件处理
├─────────────────────────────────────────┤  
│            Services Layer               │  ← 业务逻辑和数据访问
├─────────────────────────────────────────┤
│            Database Layer               │  ← Sea-ORM + SQLite
└─────────────────────────────────────────┘
```

### 工作空间结构
```
dc-license-bot/
├── src/                    # 主应用代码
│   ├── commands/           # 斜杠命令实现
│   │   ├── license/        # 许可协议相关命令
│   │   └── system.rs       # 系统管理命令
│   ├── services/           # 业务服务层
│   │   ├── license/        # 许可业务逻辑
│   │   ├── notification_service.rs  # 外部通知
│   │   ├── system_license.rs       # 系统许可缓存
│   │   └── user_settings.rs        # 用户设置
│   ├── handlers/           # Discord 事件处理
│   ├── database.rs         # 数据库连接管理
│   └── main.rs            # 应用入口
├── entities/              # 数据库实体（工作空间成员）
├── migration/             # 数据库迁移（工作空间成员）
└── config.example.toml    # 配置模板
```

### 核心技术栈
- **🦀 Rust** - 系统编程语言，高性能 + 内存安全
- **🔌 Serenity + Poise** - Discord API 封装 + 命令框架  
- **🗄️ Sea-ORM** - 现代异步 ORM，支持迁移
- **📊 SQLite** - 嵌入式数据库，简化部署
- **⚙️ Figment** - 灵活的配置管理（TOML + 环境变量）
- **🔄 Tokio** - 异步运行时
- **💾 Jemalloc** - 高性能内存分配器（减少60-80%内存使用）

## 🚀 快速开始

### 环境要求
- Rust 1.70+ 
- Discord 应用程序和机器人令牌

### 安装与配置

1. **克隆仓库**
   ```bash
   git clone https://github.com/Opizontas-Studio/dc-license-bot.git
   cd dc-license-bot
   ```

2. **配置机器人**
   ```bash
   cp config.example.toml config.toml
   # 编辑 config.toml，填入你的 Discord 机器人令牌
   ```

3. **初始化数据库**
   ```bash
   cargo run --bin migration
   ```

4. **运行机器人**
   ```bash
   cargo run -- -c config.toml -d ./data/bot.db -l ./system_licenses.json
   ```

### 配置文件示例
```toml
# Discord Bot Configuration 
# 使用前请复制为 "config.toml" 并填入信息。

# [核心配置]
token = "YOUR_DISCORD_BOT_TOKEN_HERE"
time_offset = 7200
# [安全与权限]

# 拥有特殊权限的用户ID列表。
extra_admins_ids = [
    # 80181316945921,123456789876
]
admin_role_ids = [

]


# 是否启用与备份Bot的同步功能
backup_enabled = false
# 备份Bot的接收端点 URL
endpoint = "http://127.0.0.1:8199"
```

## 📋 命令列表

### 用户命令
| 命令 | 中文名 | 描述 |
|------|--------|------|
| `/create_license` | `/创建协议` | 创建自定义许可协议 |
| `/license_manager` | `/协议管理` | 管理现有的许可协议 |
| `/publish_license` | `/发布协议` | 在帖子中发布许可协议 |
| `/auto_publish_settings` | `/自动发布设置` | 配置自动发布功能 |
| `/create_license_interactive` | `/创建协议面板` | 使用交互式面板创建新协议 |

### 管理员命令
| 命令 | 中文名 | 描述 |
|------|--------|------|
| `/system_info` | `/系统信息` | 查看系统运行状态 |
| `/reload_licenses` | `/重载系统授权` | 热重载系统许可配置 |

## 🗃️ 数据库结构

### 用户许可表 (`user_licenses`)
| 字段 | 类型 | 描述 |
|------|------|------|
| `id` | INTEGER | 许可ID（主键） |
| `user_id` | BIGINT | 用户Discord ID |
| `license_name` | TEXT | 许可协议名称 |
| `allow_redistribution` | BOOLEAN | 是否允许二次传播 |
| `allow_modification` | BOOLEAN | 是否允许二次改编 |
| `restrictions_note` | TEXT | 限制说明（可选） |
| `allow_backup` | BOOLEAN | 是否允许备份 |
| `usage_count` | INTEGER | 使用次数统计 |
| `created_at` | DATETIME | 创建时间 |

### 用户设置表 (`user_settings`)
| 字段 | 类型 | 描述 |
|------|------|------|
| `user_id` | BIGINT | 用户Discord ID（主键） |
| `auto_publish_enabled` | BOOLEAN | 是否启用自动发布 |
| `skip_auto_publish_confirmation` | BOOLEAN | 是否跳过自动发布的确认步骤 |
| `default_user_license_id` | INTEGER | 默认用户许可ID（可选） |
| `default_system_license_name` | TEXT | 默认系统许可名称（可选） |
| `default_system_license_backup` | BOOLEAN | 默认系统许可的备份设置（可选） |

### 已发布帖子表 (`published_posts`)
| 字段 | 类型 | 描述 |
|------|------|------|
| `thread_id` | BIGINT | 帖子线程ID（主键） |
| `message_id` | BIGINT | 协议消息ID |
| `user_id` | BIGINT | 发布者用户ID |
| `backup_allowed` | BOOLEAN | 当前备份权限状态 |
| `updated_at` | DATETIME | 最后更新时间 |

## 🔧 开发指南

### 本地开发
```bash
# 检查代码
cargo check

# 运行测试
cargo test

# 代码格式化
cargo fmt

# 代码检查
cargo clippy
```

### 数据库操作
```bash
# 创建新的迁移
cargo run --bin migration generate <migration_name>

# 应用迁移
cargo run --bin migration

# 重新生成实体
sea-orm-cli generate entity \
    --database-url "sqlite://./data/bot.db" \
    --output-dir entities/src/entities
```

### Docker 部署
```bash
# 构建镜像
docker build -t dc-license-bot:latest .

# 运行容器
docker run -d \
  -v $(pwd)/config.toml:/app/config.toml \
  -v $(pwd)/data:/app/data \
  dc-license-bot:latest
```

## 🔒 安全特性

- **权限验证** - 只有帖子作者可以添加许可协议
- **管理员控制** - 基于配置的管理员权限管理
- **速率限制** - 防止命令滥用的冷却机制
- **敏感信息保护** - 配置文件不包含在版本控制中

## 📈 性能优化

- **Jemalloc 内存分配器** - 显著减少内存占用（60-80%优化）
- **系统许可缓存** - 内存缓存提高响应速度
- **异步架构** - 基于 Tokio 的高并发处理
- **数据库连接池** - 优化数据库访问性能

## 🤝 贡献指南

1. Fork 项目
2. 创建功能分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

## 📄 许可证

本项目采用反商业化原则。详见项目内的许可协议声明。

## 🙏 致谢

- [Serenity](https://github.com/serenity-rs/serenity) - Discord API 库
- [Poise](https://github.com/serenity-rs/poise) - Discord 命令框架
- [Sea-ORM](https://github.com/SeaQL/sea-orm) - Rust ORM 框架

---

**DC License Bot** - 让创作者的权利得到尊重和保护 🛡️