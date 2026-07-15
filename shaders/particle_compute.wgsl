// 粒子 compute shader — GPU 端更新粒子位置

struct Global {
    resolution: vec2<f32>,
    time: f32,
    dt: f32,
};

@group(0) @binding(0) var<uniform> g: Global;
@group(0) @binding(1) var<storage, read_write> particles: array<Particle>;

struct Particle {
    pos: vec2<f32>,
    vel: vec2<f32>,
    size: f32,
    hue: f32,
};

@compute
@workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= arrayLength(&particles)) {
        return;
    }

    var p = particles[idx];
    p.pos = p.pos + p.vel * g.dt;

    // 边界反弹
    if (p.pos.x < 0.0 || p.pos.x > g.resolution.x) {
        p.vel.x = -p.vel.x;
        p.pos.x = clamp(p.pos.x, 0.0, g.resolution.x);
    }
    if (p.pos.y < 0.0 || p.pos.y > g.resolution.y) {
        p.vel.y = -p.vel.y;
        p.pos.y = clamp(p.pos.y, 0.0, g.resolution.y);
    }

    particles[idx] = p;
}
