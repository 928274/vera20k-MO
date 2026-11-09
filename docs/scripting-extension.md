# VERA20K 脚本化扩展层设计

本文件定义 VERA20K 的原生脚本/插件扩展层。它是 DynamicPatcher 思路在重写引擎上的
对应物：**保留"用脚本写单位/武器行为、引擎不用为每份模组改源码"的体验，去掉
"往黑盒 exe 里注入机器码"的全部技术债**。

首个验收目标是完整复刻一份挂在自定义单位上的多幕蓄能演出脚本（GBION 卫星定位
离子炮），见文末映射表。

## 一、目标与非目标

**目标**

1. 模组作者用脚本定义单位/武器行为，不必 fork 引擎、不必改 Rust 源码。
2. 脚本逻辑参与 sim 的确定性：可存档、可回放、可联网同步、进入 state hash。
3. 表现（激光、动画、音效）由 sim 下发指令，表现层消费，脚本不直接持有渲染对象。
4. 引擎侧扩展点稳定，新增脚本不需要改动规则读取器或存档格式。

**非目标**

- 不复刻 C# / Roslyn 运行时，也不在进程内做 inline hook、写字节、魔法地址调用。
- 不追求运行期热重载作为第一阶段交付（阶段 1 的 WASM 加载器提供，但语义先立稳）。
- 不允许脚本读任意宿主内存或阻塞主循环。

## 二、引擎现有能力盘点

以下都是当前代码中真实存在的接口，扩展层只是把它们收拢成稳定门面，不重新实现。

| 能力 | 现有实现 | 用途 |
| --- | --- | --- |
| 帧推进 | `Simulation::advance_tick`（`src/sim/world/master_frame.rs:774`） | `OnUpdate` 的挂载点 |
| 表现指令队列 | `FrameChannel::emit` / `emit_and_finish`（`src/sim/world/frame_channel.rs:23,34`） | 脚本下发表观副作用 |
| 激光 | `LifecycleOutput::LaserCreated(LaserBirth)`（`src/sim/world/lifecycle.rs:806`），表现层 `app/presentation/lasers.rs` 消费 | 激光网 / 粗光柱 |
| 坐标伤害 | `src/sim/combat/damage/receive.rs::receive_damage`；弹头解析链 `src/sim/combat/world_receiver.rs` | 主波、中心灼烧 |
| 引爆/附加效果 | `src/sim/attach_effect.rs`（`WeaponTypeExt::DetonateAt` / `WarheadTypeExt::DetonateAt` 语义已在） | 命中动画与弹头效果 |
| 超级武器 | `Simulation::fire_super_weapon(rules, owner, sw_type_id, cell, overlay, effects, automatic)`（`src/sim/superweapon/fire.rs:84`） | 全图照明 |
| 类型查表 | `RuleSet::object / weapon / warhead / super_weapon`（`src/rules/ruleset.rs:7498,7875,7977,11153`） | 按 ID 安全取类型，取代指针遍历 |
| 实体状态 | `GameEntity` 的可选状态字段（`#[serde(default)]`，如 `AttachEffects`、`ShieldState`） | 脚本实例状态的存放范式 |
| 存档/校验 | `Simulation` 的 serde + `state_hash` | 脚本状态必须纳入其中 |

**关键结论**：DP 脚本里最痛苦的部分（`CreateLaser @ 0x6FD3D0` 魔法地址、
`ABSTRACTTYPE_ARRAY` 遍历跳过 null 空位、`LaserDrawClass.Thickness` 偏移错误、
轮询 `BulletClass.Array` 判断命中），在本引擎里要么已有类型安全 API，要么已有明确事件点。
扩展层的工作是**把这些 API 收拢成可被外部脚本调用的稳定接口**。

## 三、架构：sim 层扩展点 + 指令下发

```
┌──────────────────────── sim（确定性、存档、state hash）────────────────────────┐
│  advance_tick                                                                  │
│    ├─ 收集本帧事件：Constructed / Update / WeaponFired / BulletDetonated /      │
│    │                 Destroyed                                                  │
│    └─ ScriptRegistry::dispatch(event, &mut ScriptContext)                      │
│         ├─ ScriptStateStore（挂 GameEntity，#[serde(default)]，参与 hash）      │
│         ├─ 只读视图：RuleSet、实体坐标/目标、当前帧                              │
│         └─ 副作用：伤害 / 超武 / 生成单位（走既有 sim 路径）                     │
│                    + FrameChannel<LifecycleOutput>（激光/动画/音效指令）         │
└───────────────────────────────────────────────────────────────────────────────┘
                                     │ 指令
                                     ▼
┌──────────────────────── app / presentation（非确定性）───────────────────────┐
│  lasers.rs / anim_class 投影 / audio：只消费指令，不回头影响逻辑                 │
└───────────────────────────────────────────────────────────────────────────────┘
```

**为什么扩展点放 sim 层**：伤害、触发计数、状态机推进都必须是确定性的，否则存档回放
和多人联机会分叉；纯视觉的激光/动画只是 sim 发出的指令，表现层丢帧不影响逻辑。

## 四、扩展点 trait（阶段 0，纯 Rust，先立语义）

```rust
/// 每个脚本实现在编译期注册；阶段 1 由 WASM 模块提供同一组入口。
pub trait Script: Send + Sync {
    /// 稳定 ID，写进存档；一旦发布不可变更。
    fn id(&self) -> ScriptId;

    /// 该脚本关心的事件位掩码，避免每个事件唤醒全部脚本。
    fn interests(&self) -> EventMask;

    /// 状态默认值。脚本状态是纯数据、可 serde，不含指针/句柄。
    fn initial_state(&self) -> ScriptState;

    fn on_event(&self, event: &ScriptEvent, ctx: &mut ScriptContext<'_>);
}

pub enum ScriptEvent {
    Constructed { entity: EntityId },
    Update      { entity: EntityId, frame: i32 },
    WeaponFired { entity: EntityId, weapon: InternedId, target: EntityId },
    BulletDetonated { owner: EntityId, projectile: InternedId, impact: (i32, i32, i32) },
    Damaged     { entity: EntityId, warhead: InternedId, damage: i32 },
    Destroyed   { entity: EntityId },
}
```

`ScriptContext` 只暴露受限能力，避免脚本绕开确定性：

```rust
pub struct ScriptContext<'a> {
    rules: &'a RuleSet,
    frame: i32,
    owner: InternedId,                  // 施法者所属阵营（阵营色激光用）
    anchor: (i32, i32, i32),            // 目标 GetCoords；无目标时不回落自身
    target_height: i32,                 // 目标 GetHeight()
    is_air_target: bool,
    has_target: bool,
    target_entity: Option<EntityId>,
    projectile: Option<&'a str>,        // BulletDetonated 的抛射体规则名
    commands: Vec<SimCommand>,          // 伤害/超武等，回写由宿主统一执行
    lasers: Vec<LaserBirth>,            // 纯表现，随后进入帧通道
}
```

对外方法（语义级，不含任何地址或指针）：

- `ctx.rules`：`weapon(name)` / `warhead(name)` / `super_weapon(name)` / `object(name)`
- `emit_laser(LaserBirth)` → 进入 `LifecycleOutput::LaserCreated`
- `apply_warhead(warhead, damage, coord)` → 只结算范围伤害，不产生动画/音效/光照
- `detonate(warhead, damage, coord)` → 等同 `CreateBullet(..).Detonate(coord)`：震屏、
  范围伤害、`AnimList` 动画（自带 `Report=` 音效与动画光照）、冲击闪光
- `fire_super_weapon(sw, cell)` → 走 `Simulation::fire_super_weapon`
- `nuke_flash(sw, frames)` → 以该超武类型的 `Light.*` 色调做 `ScreenNukeFlash`
- 脚本状态是 `ScriptState(Vec<u8>)`，脚本自己 serde（GBION 用 JSON），宿主只保证
  字节级持久化与确定性顺序。

**事件分发约束**：脚本只在 `advance_tick` 的固定阶段被调用；脚本产生的 `SimCommand`
在该阶段末尾按注册顺序统一执行，脚本内不得递归触发新的事件分发（避免无限递归与
同帧顺序不确定）。

## 五、插件状态与确定性

- 状态挂在 `GameEntity`：新增 `#[serde(default)] script_states: ScriptStateStore`。
  与 `AttachEffects` / `ShieldState` 同一范式，自动参与存档、加载与 state hash。
- `ScriptStateStore` 是 `ScriptId -> 定长/可 serde 的值`。阶段 0 用枚举化的原生状态，
  阶段 1 的 WASM 用 `Vec<u8>`（模块负责自身编解码，宿主只保证字节序与长度确定性）。
- 状态里**禁止**存放宿主指针、stable id 以外的裸引用、帧号以外的墙钟时间。
- 脚本不得直接调用 RNG；需要随机时走 `ScriptContext::rng()`，由宿主用既有的确定性 RNG
  记账，保证回放一致。
- 未知 `ScriptId` 的状态在加载时保留但不派发（模组移除脚本后存档仍可打开）。

## 六、WASM 接口（阶段 1）

- 运行时：`wasmtime`（或 `wasmi`），一个模块一个 `ScriptId`，线性内存按帧借出。
- ABI：模块导出 `fn interests() -> u32`、`fn on_event(event_ptr: u32, len: u32)`、
  `fn state_len() -> u32`；宿主导入 `host_read_rules`、`host_emit_laser`、
  `host_apply_warhead`、`host_fire_super_weapon`、`host_rng` 等。
- 事件与状态用固定的小端 POD 编码，避免序列化库差异；字符串一律传 interned id 的
  数值，不在 ABI 上传字符串。
- 沙箱：无 WASI、无文件系统、无时钟、内存与指令数设上限；超限按"该脚本本帧跳过"
  处理并记诊断，不影响其它脚本和主循环。
- 热重载：监视 `.wasm` 文件变更 → 校验 ABI 版本 → 在帧边界原子替换实例；状态仍由宿主
  `ScriptStateStore` 持有，因此替换模块不丢状态。

## 七、GBION 演出映射表（首个验收脚本）

| DP 脚本行为 | 本扩展层对应 |
| --- | --- |
| `[GBION] Script=...` 绑定 | `ScriptId` 注册 + 单位规则里的绑定键（阶段 0 由引擎内注册表声明） |
| `TechnoScriptable.OnUpdate()` 状态机 | `ScriptEvent::Update` + `ScriptStateStore` |
| 轮询 `BulletClass.Array` 判断 AAW 抛射体消失 | `ScriptEvent::BulletDetonated { projectile }` 直接给命中事件 |
| `SafeFind(ABSTRACTTYPE_ARRAY, ...)` 跳过 null | `RuleSet::weapon/warhead/super_weapon(id)` |
| `GameCreateLaser @ 7328272` | `ctx.emit_laser(LaserBirth)` → `LifecycleOutput::LaserCreated` |
| `LaserDrawClass.Thickness` / `IsHouseColor` | `LaserBirth.width` / `LaserColor::Explicit{ inner, outer, spread }` |
| `CreateBullet + Detonate(coord)` | `ctx.detonate(warhead, damage, coord)`（含 `AnimList` 动画与 `Report=` 音效、震屏、闪光） |
| `pSuper.Launch(cell, true)`（照明型超武） | `ctx.nuke_flash(sw, frames)`（`Light.*` 色调的地图闪光） |
| 主/副武器 `Report=` 放在枪口动画上 | 引擎既有动画 `Report=` 读取链（`art_data.rs`），无需脚本处理 |
| 4 幕状态机（旋转激光 → 蓄力 → 起爆 → 结束） | `ScriptState` 里的 `stage / radius / angle / timers / beam_flags` |

触发条件（对地：同一目标地震炮命中 2 次；对空：锁定满 60 帧）同样落在
`BulletDetonated` / `Update` 两个事件上，不再需要轮询。

## 八、分阶段路线

| 阶段 | 交付 | 验收 |
| --- | --- | --- |
| 0（本轮） | 设计文档 + `Script` trait、`ScriptEvent`、`ScriptContext`、`ScriptStateStore`；`advance_tick` 挂事件分发；`GameEntity` 状态字段 | 编译通过；一个最小脚本能收到 `Update` 并存/读状态 |
| 1 | 用阶段 0 框架完整实现 GBION 4 幕演出（原生脚本） | 游戏内演出与 DP 版一致，伤害确定性可回放 |
| 2 | WASM ABI + `wasmtime` 加载器 + 帧边界热重载 | 用外部 `.wasm` 复现同一演出，换文件即生效 |
| 3 | 模组 SDK、文档与示例 | 第三方模组能不 fork 引擎写出脚本 |

### 阶段 0a 实现状态（本轮）

- **已交付**：`src/sim/scripting.rs`（`Script` / `ScriptEvent` / `EventMask` /
  `ScriptContext` / `SimCommand` / `ScriptStateStore` / `ScriptRegistry` + 单元测试）、
  `src/sim/scripting/source.rs`（脚本目录发现、清单解析、诊断）、
  `Simulation::script_states` 字段（`#[serde(default)]`，存档兼容）、
  `ObjectType::script`（`Script=` 读取）、`Script::binding_name` /
  `ScriptRegistry::binding_for`（名字 → ScriptId）、本文档与 `.gitignore` 白名单。
- **与设计的一处偏差**：状态放在 **Simulation 级 store**（按 `(stable_id, ScriptId)`
  索引），未下沉到 `GameEntity`。`GameEntity` 构造/快照点极多，下沉会带来大范围改动与
  存档兼容风险；Simulation 级 store 语义等价、改动面小，将来可无痛迁移。
- 因此阶段 0a **不改变任何帧行为**：注册表为空、store 为空，现有游戏与测试逐字节不变。

### 阶段 0b 实现状态

`src/sim/scripting/runtime.rs` + 四处接入，**已全部完成**：

1. **state hash**：在真正的确定性折叠 `Simulation::fold_state()` 末尾追加
   `script_states.checksum().hash(hasher)`。`state_hash_owners()` 只是只读诊断投影
   （固定 15 项，且被 `hash_owner_diagnostics_for_test()` 的 `[1..]` 切片依赖），
   因此**不动该数组**，快照测试不受影响；追加在末尾也不改变任何已有折叠结果。
2. **自动派发**：`advance_master_frame` 在 `LOGIC_STEPS` 之后、`run_late_region`
   之前调用 `dispatch_script_frame()`。它按 stable-id 顺序收集声明了已注册
   `Script=` 的实体，逐个派发 `Update`；顺序固定，可回放、可联机。
   **注册表为空时立即返回，零每帧开销**。
3. **SimCommand 宿主执行**：
   - `LaserBirth` → `lifecycle_outputs.emit(LifecycleOutput::LaserCreated)`（纯表现）✅
   - `FireSuperWeapon` → `Simulation::fire_super_weapon(..., automatic=true)` ✅
   - `ApplyWarhead` → **本阶段输出结构化 `log::debug!` 诊断，不施加伤害**。
     完整链路 `combat::world_receiver::apply_area_damage` 的 `origin` 三元组与
     `affects_tiberium` 语义需要结合首个真实脚本（GBION 演出）实测确定，
     猜测着接线会让回放分叉；留到阶段 1 与演出一起接通。
4. **Destroyed 清理**：已在 `ScriptRegistry::dispatch` 内实现。
5. **启动发现**：`app/initialize.rs` 在拿到 `game_config` 后调用
   `ScriptDirectory::discover(&config.paths.ra2_dir)`，把模块数与全部 diagnostics
   以 `log::info!` 输出；目录异常只记日志，绝不阻止启动。

进程级注册表（`script_registry()` / `install_script_registry()`）已就位；阶段 1 的
原生脚本、阶段 2 的 WASM 加载器都往同一个注册表安装。

**仍未改变帧行为**：注册表为空 → `dispatch_script_frame` 直接返回；`script_states`
为空 → state hash 追加的是一个常量。用户可见的唯一变化是启动日志里多一条脚本目录报告。

## 九、脚本目录约定

**根目录**：`<游戏目录>/vera20k/DynamicPatcher/`（`src/sim/scripting/source.rs` 的
`SCRIPT_DIR` 常量，相对 `ra2_dir`）。这个位置与 VERA 自己的
`<游戏目录>/vera20k/` 用户数据目录同级，也**刻意区别于**顶层原版 DynamicPatcher 的
`<游戏目录>/DynamicPatcher/` —— 两套目录可以共存、互不干扰。

调用方可以改用 `ScriptDirectory::discover_at(root)` 覆盖根路径，因此该约定是
**可配置的默认值**，不是写死的唯一路径。

### 目录布局（扁平 + 清单）

```
<游戏目录>/vera20k/DynamicPatcher/
├── scripts.json                 ← 清单：脚本名 ↔ 模块文件
├── gbion_ion_show.wasm          ← 将来的 WASM 模块，直接平铺
├── other_script.wasm
└── GBIONIonShowScript.cs        ← 现有 DP 源码，留作迁移参考，引擎不加载
```

```json
{
  "scripts": [
    { "name": "GBIONIonShowScript", "module": "gbion_ion_show.wasm" }
  ]
}
```

- `name`：与单位规则里的 `[Unit] Script=` 值大小写不敏感匹配。
- `module`：相对本目录的文件名，必须是 `.wasm`。

### 绑定闭环

```
[GBION] Script=GBIONIonShowScript        ← 规则层读进 ObjectType::script
        ↓
scripts.json  name → module              ← ScriptDirectory::module_for()
        ↓
ScriptRegistry::binding_for(name)        ← 解析成 ScriptId
        ↓
ScriptStateStore / 事件派发              ← 实例状态 + 每帧更新
```

`Script=` 由 `ObjectType::script` 统一读取，**不硬编码任何具体脚本名**；未注册的
名字只产生诊断，绝不阻止类型加载（与之前删掉的 no-op `Script=` 空桩不同，
这次有真实消费者 `ScriptRegistry::binding_for`）。

### 发现行为（阶段 0a）

`ScriptDirectory::discover()` 只做只读扫描与校验，**不加载、不执行**：

- 目录不存在 / `scripts.json` 缺失 / JSON 非法 / 清单引用了不存在的模块
  → 记入 `diagnostics`，游戏照常启动；
- `*.cs` → 标为 `LegacyCSharp`，提示"需迁移成 WASM 模块才能运行"；
- `*.wasm` → 标为 `Module`，是阶段 2 加载器的候选；
- 文件列表排序后输出，保证诊断在不同文件系统上一致。

**注意**：阶段 0/1 的脚本仍是编译进引擎的原生 Rust，此时把文件放进目录**不会生效**；
真正从目录加载 WASM 是阶段 2。

### 阶段 1 实现状态（首个原生脚本）

`src/sim/scripting/gbion.rs` 完整迁移了四幕演出，`runtime.rs` 把命令接到了真实
sim 入口，`initialize.rs` 在启动时 `install_builtin_scripts()` 安装注册表。

- **状态机**：`Show`（serde → `ScriptState`）保存 `stage / delay / radius / angle /
  line_speed / rot_speed / ready / charge / beams[8]`，加上触发与锚点状态
  `quake_hits / quake_target / air_frames / air_target / center / height / air`。
  四幕 = 旋转激光收缩 → 蓄力 → 起爆 → 结束，每帧由 `Update` 推进。
- **触发（与 DP 脚本一致，无兜底）**：
  1. 本单位自己的 `AAW` 抛射体（`EarthQuake` 的 `Projectile=AAW`）引爆 2 次，
     且属于同一目标 → 触发；目标切换/丢失清零计数。
  2. 持续锁定飞行目标（`GetHeight() > 0`）满 60 帧 → 触发。
  `BulletDetonated` 由 `commit_projectile_detonations_inline` 写入
  `Simulation::script_bullet_hits`（单帧通道，不存档不入 hash），
  `dispatch_script_frame` 在 `Update` 之前按引爆顺序派发。
  **建造完成/刚出生不再有任何演出**——首版的 90 帧 fallback 已删除，
  它正是"建造中就放核爆"的根因。
- **锚点**：`center / height` 锁存在脚本状态里。触发时取目标 `GetCoords`（无目标
  时取那次引爆的 `impact`），演出中每帧若仍持有目标就刷新，目标丢失则停在最后
  位置。**永远不会回落到施法者自身坐标**，所以落点不可能落在自己头上。
  `ScriptContext` 的 `has_target` / `target_entity` 如实反映 `TarCom`。
- **表现映射**：第 1 幕 8 道细光束用 `LaserColor::Explicit` 蓝白配色（width 4，
  原生 General 臂不按 thickness 加宽，与原脚本一致）；第 3 幕 3×3 粗光柱用
  `LaserColor::House(owner)` + width 40 —— 原脚本是 `IsHouseColor=true`，
  **只有阵营色臂（Special）才按 thickness 逐圈加宽**，这是粗光柱此前偏细的原因。
  光柱与核爆统一 snap 到 `Cell2Coord(Coord2Cell(center))`，与原生对齐方式相同；
  `nuke_flash("GBIONBlueLight", 100)` 对应蓝色全图照明；中心灼烧 / 蓄力 / 主波
  全部走 `detonate`，动画与音效由弹头自己的 `AnimList=`（`GBIONLSR` /
  `NUKEANIM` / `TWLT100`）和动画 `Report=` 提供。
- **真实执行**：`SimCommand::Detonate` 复刻 `BulletClass::DetonateAtCoord` 的次序——
  `ShakeX/Y` 主 RNG 抽两次 → `apply_area_damage` → `detonation_anim::effect`
  （`SelectAnim` + `admit_explosion_effect`，动画 Start 时发 `AnimationStarted`
  音效事件）→ `CombatLightRequest::Impact`。`ApplyWarhead` 保留为纯伤害版本。
  `FireSuperWeapon` 仍走 `Simulation::fire_super_weapon`。
- **命令改用规则名字符串**（不再是 `InternedId`），宿主执行时才 intern ——
  脚本层因此不需要接触 interner，也更容易跨 WASM ABI 传递。
- `ScriptContext` 另有 `owner` / `projectile`：`owner` 供 `LaserColor::House`
  取阵营色，`projectile` 是该次 `BulletDetonated` 的抛射体规则名（脚本据此只
  统计 `AAW`）。

**仍与 DP 版有差异**：粗光柱的实际像素宽度取决于阵营色的亮度（`LaserDraw`
阵营色臂每加宽一圈把 side 色右移一位，过暗即停），换浅色阵营会更粗；
DP 版还会给中心光束挂 `GBIONION` 枪口动画（`Anim=GBIONION`，自带 `Report`），
本移植的 `EarthQuake` 开火表现由武器本身的 `Anim=` 提供，未额外补。

## 十、风险与约束

- **state hash 兼容**：新增 `GameEntity` 字段会改变存档/校验布局，需要同步更新快照测试
  与存档版本；旧存档靠 `#[serde(default)]` 兼容。
- **事件风暴**：大量单位每帧派发 `Update` 有成本，用 `EventMask` + 仅对绑定了脚本的实体
  派发来约束。
- **同帧顺序**：`SimCommand` 必须按固定顺序执行，禁止脚本内递归分发。
- **不重复造轮子**：激光、伤害、超武、动画全部复用既有实现，扩展层只做门面与生命周期；
  若上游后续增加同类能力，优先迁移到上游接口。
- **阶段 0 不含 WASM**：先把扩展点语义与确定性模型立稳，再接 ABI，避免语义返工。
