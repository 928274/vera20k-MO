# MO 适配分支说明

本仓库（`928274/vera20k-MO`，分支 `mo`）是 [YuriPlanet/vera20k](https://github.com/YuriPlanet/vera20k)
的下游 fork，目标是在 VERA20K 重写引擎上运行 Mental Omega MOD。
它不是上游的功能分支：所有差异都集中在「让 MO 的资源与规则能被正确加载和运行」，
引擎本身的通用能力仍持续跟随上游。

## 与上游的关系

| 项目 | 值 |
| --- | --- |
| 当前合并基线 | `c8a3c916d`（tag `v0.2.8`，开发版 0.2.9） |
| 上一次 rebase 基线 | `f5a28350f` |
| 本次整合方式 | `git merge --no-ff upstream/main`，保留双方历史，不改写已推送历史 |
| 上游相对旧基线 | 29 个提交，196 文件，+8772 / −1631 |
| 本 fork 特有 | 10 个提交，76 文件，+3242 / −262 |

整合时 16 个双方都修改的文件中 13 个自动合并成功，3 个内容冲突
（`docs/ini-directives.md`、`src/rules/ini_directives.rs`、`src/rules/ini_parser.rs`），
全部集中在 INI 语法层，解决方式见下文「与上游功能重叠的部分」。

## 本 fork 特有的 10 个提交

| 提交 | 类别 | 内容 |
| --- | --- | --- |
| `bad857e1e` | 规则 / INI | 散文件根加载、`[#include]` 重复追加键、`[CHILD]:[PARENT]` 继承、`NOSTR:`、根选择、13 色 skirmish 颜色、双模式指针、数字血量调色板回退、Sight 上限 |
| `335cfbbb1` | 模拟 / 玩法 | Ares `Convert.Deploy`、`NoAmmoWeapon`、`CloningFacility`、`InitialPayload`、中立建筑占领、`PoweredSpecial`、SHP 建筑炮塔、BGM 自动播放、八角形视野 |
| `139ebdb70` | 渲染 / 表现 | 军衔 insignia 图集按名绑定、MCV 部署调色板、炮塔 slot 9 绘制、自定义指针、围墙阵营色烘焙 |
| `a458a396f` | 应用接入 | CnCNet 启动链路、音频失败回退、架构守卫 pin 表、新增 SHP 炮塔/自定义调色板文档 |
| `47db8fe33` | 编译修正 | 适配上游签名变更（建筑动画 slot、指针枚举、软件光标） |
| `4a4831376` | bug 修复 | 矿石/宝石不可见：恢复 overlay sprite key 的 tiberium 分支 |
| `7601b8870` | bug 修复 | 建造厂开局 build-up 反复播放：落地后立即 apply Idle body |
| `fb24e3031` | bug 修复 | 超级武器 `SW.ShowCameo=no` 缩略图泄漏；武器 `Anim=` / 弹头 `AnimList=` 恢复 Find-or-Allocate |
| `1bc3b4bd3` | 清理 | 删除临时的 `Script=` 兼容骨架（no-op 桩，非引擎能力） |
| `1c886ec85` | 清理 | 文档与测试中的具体 MOD 名称泛指化为中性命名 |

## 上游没有、必须保留的适配点

以下能力在上游 `c8a3c916d` 中不存在（按符号检索确认），是本 fork 存在的主要理由：

- `ares_tolerant_root_is_usable`：容忍 AI/规则根文件中的悬空引用，否则 MO 无法进入游戏。
- `expandmo##.mix` / `thememo.ini` / `artmo` / `aimo` 根加载与根选择。
- `SkirmishColorMode::MENTAL_OMEGA`（13 色）。
- Sight 上限放宽（`REVEAL_RADIUS_MAX`）与八角形视野分支。
- BGM 自动播放、CnCNet 启动链路、SHP 建筑炮塔、军衔 insignia、围墙阵营色、
  `InitialPayload`、中立磁能球体等 MO 玩法与表现适配。
- 三个游戏 bug 修复（矿石、建造厂、cameo / 武器动画列表）。

## 与上游功能重叠的部分（整合重点）

上游 `56fa9bb59 "Ares INI syntax: += keys, [Child]:[Base] sections and later-wins keys (#4130)"`
在解析层新增了 `src/rules/ini_ares_syntax.rs`：`+=` 会被展开成 `var_<n>` 键，
`[Child]:[Base]` 继承与 later-wins 重复键也在 `IniFile::from_str/from_bytes` 统一处理。
这与本 fork `bad857e1e` 早期的 `APPEND_KEY` / `physical_entries` /
`apply_ares_section_inheritance` 实现功能重叠。

本次整合按「保留本 fork 实现」处理：

- `include_list()` 采用上游的 `Vec<String>` 签名与 `load_listed()` 调用链
  （否则类型不匹配），但保留本 fork 对字面 `+` 键的逐条 stored-value 读取
  （`APPEND_KEY` + `strtrim_ascii` / `truncate_native_bytes`），作为未经过
  Ares 语法展开的 INI 的兜底路径。
- `ini_parser.rs` 的 `parse_line()` / `ares_header_parent()` 与
  `ini_directives.rs` 的 `apply_ares_section_inheritance()` 原样保留，
  前两者标记为 `#[allow(dead_code)]`：Ares 构建现在走
  `ares_syntax::load_fresh()`，不再经过这条逐行解析路径。
- `header_parent` 字段保留，供 directive-pass 继承逻辑读取。

**注意**：这是两套实现并存的状态。Ares 构建下 `+=` 与 `[Child]:[Base]`
理论上由上游解析层处理，本 fork 的兜底分支只在遇到字面 `+` 键时生效。
仍需实测确认没有双重展开，尤其是 `[#include] +=` 与类型注册表 `+=` 两类列表。

## 构建与部署

必须使用 Ares + Phobos feature，且禁用默认 feature：

```sh
cargo build --release --no-default-features --features ares,phobos --bin vera20k
```

Windows 上需先准备 MSVC 14.44 / Windows SDK 10.0.22621 的 `PATH` / `INCLUDE` / `LIB` /
`CC` / `CXX` 环境变量。产物为 `target/release/vera20k.exe`，部署时复制到游戏目录
（替换前建议备份旧 exe）。编译收尾若出现 `reg.exe` 被安全策略拦截，属于编译完成后的
磁盘检查，不影响产物。

## 已知待验证项

- 上游本次新增的 Tunnel 地下移动、Carryall 吊运、色觉滤镜、DX12 只读着色器修复等
  功能，尚未在 MO 环境下实测。
- INI 语法层两套实现并存（见上），需验证 `+=` / `[Child]:[Base]` 没有被双重处理。
- 后续每次合并上游时，需重新检查上表「上游没有、必须保留的适配点」是否被上游吸收，
  一旦上游原生支持，应删除本 fork 的对应实现。
