// ============================================================================
// GBION 卫星定位离子炮 —— 演出脚本（Rhai）
//
// 放置位置：<游戏目录>/vera20k/DynamicPatcher/GBIONIonShowScript.rhai
// 绑定方式：文件名即 [Unit] Script= 的名字（大小写不敏感），本脚本对应
//           gbionrules.ini 里的 Script=GBIONIonShowScript。
// 生效方式：保存本文件后，再造一个 GBION 单位即读取新内容（不用重启、不用重编译）。
//
// 引擎注入的东西只有三样：
//
//   ctx   只读事实表
//         ctx.event       "update" / "detonate" / "constructed"
//         ctx.frame       当前帧号
//         ctx.has_target  当前是否有攻击目标
//         ctx.air         目标是空中单位
//         ctx.height      目标的 GetHeight()（地面单位 0）
//         ctx.anchor      目标坐标 [x, y, z]，单位 lepton（1 格 = 256）
//         ctx.target      目标实体 id（无目标为 0）
//         ctx.projectile  引爆的抛射体规则名（仅 detonate）
//         ctx.impact      引爆点坐标（仅 detonate）
//
//   state 本单位的脚本状态（会存档、会进 state hash），默认值见 default_state()
//
//   宿主函数
//         laser(from, to, width, duration)              阵营色光柱，只有它按 width 加宽
//         laser_rgb(from, to, width, duration, [r,g,b]) 指定颜色（细线用，宽度不生效）
//         detonate(warhead, damage, coord)              = DP 的 CreateBullet(..).Detonate()：
//                                                       伤害 + 弹头 AnimList 动画（自带 Report 音效）
//                                                       + 震屏 + 冲击光
//         damage(warhead, damage, coord)                只结算伤害，不出动画/音效
//         flash(superweapon, frames)                    以该超武 Light.* 色调做全图闪光
//         superweapon(name, cell_x, cell_y)             真正发射超武
//         log(text)                                     写日志
//         ring_point(center, radius, degrees)           圆周上一点 [x,y,z]
//         cos_int(radius, degrees) / sin_int(...)       整数化三角
//         cell_center(lepton)                           Cell2Coord(Coord2Cell(x))，按格对齐
//
// 注意：Rhai 的函数看不到文件顶层的 const，所以可调参数统一放在 cfg() 里，
//       各幕开头 `let cfg = cfg();` 取一份。改数值只改 cfg()。
// ============================================================================

// ---------------------------- 可调参数 --------------------------------------
fn cfg() {
    #{
        // 粗光柱（第 3 幕）。原 DP 脚本：Thickness=40、3x3 偏移 ±20，
        // 武器 [GBIONIonBigLaser] LaserDuration=30 / LaserThickness=150。
        column_width: 40,            // 每条光柱的发光宽度（像素级），越大越粗
        column_duration: 30,         // 光柱存活帧数（=武器 LaserDuration）
        column_height: 50000,        // 光柱起点高度（lepton）
        column_offsets: [-20, 0, 20],// 3x3 网格偏移（lepton，256=1 格）

        // 细激光（第 1 幕）
        beam_width: 4,               // 细线宽度（显式配色下不生效，与原脚本一致）
        beam_duration: 2,            // 细激光存活帧数
        beam_color: [0, 35, 180],    // [GBIONIonLaser] LaserInnerColor
        beam_height: 50000,

        // 触发与节奏
        quake_projectile: "AAW",     // 地震炮抛射体：对同一目标命中 2 次开演
        quake_hits: 2,
        air_frames: 60,              // 锁定飞行目标满 60 帧开演（地震炮打不到空中）
        start_delay: 10,             // 开演延迟
        start_radius: 166,           // 起始半径
        start_angle: -180,
        line_speed: 24,
        rot_speed: 8,

        // 弹头与超武
        beam_warhead: "GBIONBeamLowWH",
        charge_warhead: "GBIONChargeWH",
        wave_warhead: "GBIONWaveWH",
        wave_air_warhead: "GBIONWaveAirWH",
        blue_light: "GBIONBlueLight",
        blue_light_frames: 100       // [GBIONLightWH] NukeFlash.Duration=100
    }
}

// ---------------------------- 状态 ------------------------------------------
fn default_state() {
    let cfg = cfg();
    #{
        active: false,
        stage: 0,                    // 0 空闲 / 1 旋转收缩 / 2 蓄力 / 3 起爆 / 4 结束
        delay: 0,
        radius: cfg.start_radius,
        angle: cfg.start_angle,
        line_speed: cfg.line_speed,
        rot_speed: cfg.rot_speed,
        ready: 0,                    // 逐根点亮进度
        charge: 0,
        beams: [false, false, false, false, false, false, false, false],
        quake_hits: 0,               // 当前目标上的地震炮命中数
        quake_target: 0,             // 命中计数属于哪个目标
        air_frames: 0,               // 锁定飞行目标的帧数
        air_target: 0,
        center: [0, 0, 0],           // 锁定的目标坐标（lepton）
        height: 0,                   // 目标高度：地面落点 = center[2] - height
        air: false
    }
}

// ---------------------------- 事件 ------------------------------------------

// 本单位发射的抛射体引爆一次。原脚本轮询 BulletClass.Array 等 AAW 消失，
// 引擎这里直接给出每一次引爆。
fn on_detonate(ctx, state) {
    let cfg = cfg();
    if state.active || ctx.projectile != cfg.quake_projectile {
        return state;
    }
    if state.quake_target != ctx.target {
        state.quake_hits = 0;
        state.quake_target = ctx.target;
    }
    if ctx.has_target {
        state.center = ctx.anchor;
        state.height = ctx.height;
        state.air = ctx.air;
    } else {
        // 这一发已经把目标打死了：引爆点是唯一可信的中心，绝不是施法者自己。
        state.center = ctx.impact;
        state.height = 0;
        state.air = false;
    }
    state.quake_hits += 1;
    if state.quake_hits >= cfg.quake_hits {
        state = start_show(state);
    }
    state
}

fn on_update(ctx, state) {
    let cfg = cfg();
    if state.active {
        // 每帧重读目标坐标：演出跟着目标走；目标没了就停在原地。
        if ctx.has_target {
            state.center = ctx.anchor;
            state.height = ctx.height;
            state.air = ctx.air;
        }
        if state.delay > 0 {
            state.delay -= 1;
            return state;
        }
        if state.stage == 1 {
            state = act_beams(state);
        } else if state.stage == 2 {
            state = act_charge(state);
        } else if state.stage == 3 {
            state = act_release(state);
        } else {
            state.active = false;
            state.stage = 0;
        }
        return state;
    }

    // 空闲：命中计数不跨目标。
    if state.quake_target != ctx.target {
        state.quake_hits = 0;
        state.quake_target = ctx.target;
    }
    // 第二条触发：地震炮打不到空中，所以锁定飞行目标满 60 帧也开演。
    if ctx.has_target && ctx.air {
        if state.air_target != ctx.target {
            state.air_frames = 0;
            state.air_target = ctx.target;
        }
        state.air_frames += 1;
        if state.air_frames >= cfg.air_frames {
            state = start_show(state);
        }
    } else {
        state.air_frames = 0;
        state.air_target = 0;
    }
    state
}

// ---------------------------- 各幕 ------------------------------------------

fn start_show(show) {
    let cfg = cfg();
    show.active = true;
    show.stage = 1;
    show.delay = cfg.start_delay;
    show.radius = cfg.start_radius;
    show.angle = cfg.start_angle;
    show.line_speed = cfg.line_speed;
    show.rot_speed = cfg.rot_speed;
    show.ready = 0;
    show.charge = 0;
    show.beams = [false, false, false, false, false, false, false, false];
    show.quake_hits = 0;
    show.air_frames = 0;
    show
}

// 第 1 幕：8 道细激光逐根亮起（1→2→4→3→6→8→7、5），然后旋转收缩、逐渐加速。
fn act_beams(show) {
    let cfg = cfg();
    if show.ready <= 60 {
        show.ready += 2;
        show.beams[0] = true;
        if show.ready >= 15 { show.beams[1] = true; }
        if show.ready >= 30 { show.beams[3] = true; }
        if show.ready >= 40 { show.beams[2] = true; }
        if show.ready >= 45 { show.beams[5] = true; }
        if show.ready >= 55 { show.beams[7] = true; }
        if show.ready >= 60 {
            show.beams[6] = true;
            show.beams[4] = true;
        }
    }

    let center = show.center;
    let floor_z = center[2] - show.height;
    let index = 0;
    while index < 8 {
        if show.beams[index] {
            let point = ring_point(center, show.radius, show.angle + index * 45);
            let x = point[0];
            let y = point[1];
            laser_rgb([x, y, center[2] + cfg.beam_height], [x, y, floor_z],
                      cfg.beam_width, cfg.beam_duration, cfg.beam_color);
            // 逐根亮起 20 伤，旋转扫射阶段 120 伤（会扫到周围单位）。
            let damage = if show.ready > 60 { 120 } else { 20 };
            detonate(cfg.beam_warhead, damage, [x, y, floor_z]);
        }
        index += 1;
    }

    if show.ready >= 60 {
        if show.ready % 5 == 0 {
            show.line_speed += 1;
            show.rot_speed += 1;
        }
        show.radius -= show.line_speed;
        show.angle -= show.rot_speed;
        // 中心持续灼烧，不依赖激光扫到中心。
        detonate(cfg.beam_warhead, 100, [center[0], center[1], floor_z]);
        if show.radius < 0 {
            show.stage = 2;
        }
    }
    show
}

// 第 2 幕：蓄力，第 5 帧出零伤蓄力动画。
fn act_charge(show) {
    let cfg = cfg();
    if show.charge == 5 {
        let center = show.center;
        detonate(cfg.charge_warhead, 0, [center[0], center[1], center[2] - show.height]);
    }
    show.charge += 1;
    if show.charge >= 20 {
        show.stage = 3;
    }
    show
}

// 第 3 幕：全图蓝光 + 3x3 粗光柱 + 主波核爆。
fn act_release(show) {
    let cfg = cfg();
    let center = show.center;
    let floor_z = center[2] - show.height;
    // 光柱与核爆统一对齐到格心，和原脚本的 Cell2Coord(Coord2Cell(center)) 一致。
    let cell_x = cell_center(center[0]);
    let cell_y = cell_center(center[1]);

    // 全图蓝色照明：原脚本发 MultiMissile 超武，实际只用到它的闪光。
    flash(cfg.blue_light, cfg.blue_light_frames);

    for ox in cfg.column_offsets {
        for oy in cfg.column_offsets {
            let x = cell_x + ox;
            let y = cell_y + oy;
            laser([x, y, center[2] + cfg.column_height], [x, y, floor_z],
                  cfg.column_width, cfg.column_duration);
        }
    }

    if show.air {
        detonate(cfg.wave_air_warhead, 700, [cell_x, cell_y, center[2]]);
    }
    // NUKEANIM + ShakeX/Y=20：核爆动画、核爆音效、屏幕抖动都来自这一次引爆。
    detonate(cfg.wave_warhead, 700, [cell_x, cell_y, floor_z]);

    show.stage = 4;
    show
}
