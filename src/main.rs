//! Gallery3D — a small first-person macroquad demo.
//!
//! Walk through a 3-room art gallery (arrow keys to move/turn) and view a
//! sampling of renders from a ray tracer project as framed wall paintings.
//! Lighting is baked per-vertex on the CPU (ambient + attenuated point
//! lights) since macroquad's default 3D pipeline has no built-in lighting.

use macroquad::miniquad::window::set_mouse_cursor;
use macroquad::miniquad::CursorIcon;
use macroquad::prelude::*;

// ---------------------------------------------------------------------
// Level layout constants
// ---------------------------------------------------------------------

const ROOM_DEPTH: f32 = 10.0; // size along Z
const ROOM_WIDTH: f32 = 8.0; // size along X, per room
const NUM_ROOMS: usize = 3;
const ROOM_HEIGHT: f32 = 4.0;
const LEVEL_LENGTH: f32 = ROOM_WIDTH * NUM_ROOMS as f32;

const DOOR_Z_MIN: f32 = ROOM_DEPTH / 2.0 - 1.3;
const DOOR_Z_MAX: f32 = ROOM_DEPTH / 2.0 + 1.3;
const WALL_THICKNESS_COLLIDE: f32 = 0.22;
// Dividing walls get a separate, differently-colored face per adjoining
// room; nudging each face slightly into its own room keeps the two from
// being perfectly coplanar (which would otherwise z-fight).
const DIVIDER_EPS: f32 = 0.01;

const EYE_HEIGHT: f32 = 1.65;
const MOVE_SPEED: f32 = 4.2;
const TURN_SPEED: f32 = 1.9; // radians / sec
const PLAYER_MARGIN: f32 = 0.35;

// ---------------------------------------------------------------------
// Lighting
// ---------------------------------------------------------------------

struct PointLight {
    pos: Vec3,
    color: Vec3,
    intensity: f32,
}

struct Lighting {
    ambient: Vec3,
    lights: Vec<PointLight>,
}

impl Lighting {
    fn shade(&self, pos: Vec3, normal: Vec3) -> Color {
        let mut acc = self.ambient;
        for light in &self.lights {
            let delta = light.pos - pos;
            let dist = delta.length().max(0.05);
            let ndir = delta / dist;
            let ndotl = normal.dot(ndir).max(0.0);
            let atten = light.intensity / (1.0 + 0.22 * dist + 0.06 * dist * dist);
            acc += light.color * ndotl * atten;
        }
        Color::new(
            acc.x.clamp(0.0, 1.0),
            acc.y.clamp(0.0, 1.0),
            acc.z.clamp(0.0, 1.0),
            1.0,
        )
    }
}

/// Combine a surface's base color with its lit color, boosted slightly
/// (`BOOST`) since the texture multiply plus ambient otherwise reads dark.
const BOOST: f32 = 1.6;

fn shade_bytes(base: Color, lit: Color) -> [u8; 4] {
    [
        ((base.r * lit.r * BOOST).clamp(0.0, 1.0) * 255.0) as u8,
        ((base.g * lit.g * BOOST).clamp(0.0, 1.0) * 255.0) as u8,
        ((base.b * lit.b * BOOST).clamp(0.0, 1.0) * 255.0) as u8,
        255,
    ]
}

// ---------------------------------------------------------------------
// Mesh building helpers
// ---------------------------------------------------------------------

/// Build a subdivided quad on a plane, with lighting baked per-vertex.
/// `origin` is the corner, `u_axis`/`v_axis` span the quad (already scaled
/// to full size), `normal` is used for lighting, `segments` controls
/// subdivision (for smoother light falloff), and `uv_tiles` controls how
/// many times the texture repeats across the quad (independent per cell,
/// so no texture-wrap support is required).
#[allow(clippy::too_many_arguments)]
fn build_quad(
    out: &mut Vec<Vertex>,
    out_idx: &mut Vec<u16>,
    origin: Vec3,
    u_axis: Vec3,
    v_axis: Vec3,
    normal: Vec3,
    segments: (u32, u32),
    base_color: Color,
    lighting: &Lighting,
) {
    let (su, sv) = segments;
    let base_vtx = out.len() as u16;
    for j in 0..=sv {
        for i in 0..=su {
            let fu = i as f32 / su as f32;
            let fv = j as f32 / sv as f32;
            let pos = origin + u_axis * fu + v_axis * fv;
            let lit = lighting.shade(pos, normal);
            out.push(Vertex {
                position: pos,
                uv: vec2(fu, fv),
                color: shade_bytes(base_color, lit),
                normal: vec4(normal.x, normal.y, normal.z, 0.0),
            });
        }
    }
    let stride = su + 1;
    for j in 0..sv {
        for i in 0..su {
            let a = base_vtx + (j * stride + i) as u16;
            let b = base_vtx + (j * stride + i + 1) as u16;
            let c = base_vtx + ((j + 1) * stride + i + 1) as u16;
            let d = base_vtx + ((j + 1) * stride + i) as u16;
            out_idx.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }
}

/// A single flat-lit quad (used for picture frames & paintings), sampled
/// once at its center for a uniform, gallery-spotlight look.
fn build_flat_quad(
    out: &mut Vec<Vertex>,
    out_idx: &mut Vec<u16>,
    origin: Vec3,
    u_axis: Vec3,
    v_axis: Vec3,
    normal: Vec3,
    base_color: Color,
    lighting: &Lighting,
) {
    let center = origin + u_axis * 0.5 + v_axis * 0.5;
    let lit = lighting.shade(center, normal);
    let color = shade_bytes(base_color, lit);
    let base_vtx = out.len() as u16;
    let corners = [
        (origin, vec2(0.0, 1.0)),
        (origin + u_axis, vec2(1.0, 1.0)),
        (origin + u_axis + v_axis, vec2(1.0, 0.0)),
        (origin + v_axis, vec2(0.0, 0.0)),
    ];
    for (pos, uv) in corners {
        out.push(Vertex {
            position: pos,
            uv,
            color,
            normal: vec4(normal.x, normal.y, normal.z, 0.0),
        });
    }
    out_idx.extend_from_slice(&[
        base_vtx,
        base_vtx + 1,
        base_vtx + 2,
        base_vtx,
        base_vtx + 2,
        base_vtx + 3,
    ]);
}

// ---------------------------------------------------------------------
// Procedural textures (no external files needed besides gallery photos)
// ---------------------------------------------------------------------

/// Each room gets its own accent wall color, like a gallery that repaints
/// its walls between exhibits.
fn room_color(room: usize) -> Color {
    match room % 3 {
        0 => Color::new(0.80, 0.60, 0.54, 1.0), // warm terracotta
        1 => Color::new(0.56, 0.67, 0.78, 1.0), // cool slate blue
        _ => Color::new(0.60, 0.74, 0.58, 1.0), // sage green
    }
}

fn make_wall_texture(tint: Color) -> Texture2D {
    let size = 128u16;
    let mut img = Image::gen_image_color(size, size, tint);
    for y in 0..size {
        for x in 0..size {
            let n = ((x as f32 * 12.9898 + y as f32 * 78.233).sin() * 43758.5453).fract().abs();
            let shade = 0.96 + n * 0.06;
            img.set_pixel(
                x as u32,
                y as u32,
                Color::new(tint.r * shade, tint.g * shade, tint.b * shade, 1.0),
            );
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

fn make_floor_texture() -> Texture2D {
    let size = 128u16;
    let mut img = Image::gen_image_color(size, size, Color::new(0.35, 0.22, 0.14, 1.0));
    let plank_h = size / 8;
    for y in 0..size {
        for x in 0..size {
            let plank_idx = y / plank_h.max(1);
            let offset = if plank_idx % 2 == 0 { 0 } else { size / 3 };
            let seam = (x + offset) % (size / 3) < 1 || (y % plank_h.max(1)) < 1;
            let n = ((x as f32 * 3.1 + y as f32 * 9.7).sin() * 10000.0).fract().abs();
            let shade = 0.9 + n * 0.18;
            let base = if seam {
                Color::new(0.20, 0.12, 0.07, 1.0)
            } else {
                Color::new(0.42 * shade, 0.27 * shade, 0.16 * shade, 1.0)
            };
            img.set_pixel(x as u32, y as u32, base);
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

fn make_ceiling_texture() -> Texture2D {
    let size = 64u16;
    let mut img = Image::gen_image_color(size, size, Color::new(0.93, 0.93, 0.95, 1.0));
    for y in 0..size {
        for x in 0..size {
            let grid = x % (size / 4) == 0 || y % (size / 4) == 0;
            let c = if grid {
                Color::new(0.85, 0.85, 0.88, 1.0)
            } else {
                Color::new(0.94, 0.94, 0.96, 1.0)
            };
            img.set_pixel(x as u32, y as u32, c);
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

// ---------------------------------------------------------------------
// Level construction
// ---------------------------------------------------------------------

struct Painting {
    texture: Texture2D,
    #[allow(dead_code)]
    label: &'static str,
}

struct Level {
    opaque_meshes: Vec<Mesh>,  // one mesh per room's walls, plus one floor and one ceiling mesh
    painting_meshes: Vec<Mesh>, // one mesh per painting (own texture)
    fixtures: Vec<(Vec3, f32, Color)>, // light fixture visuals: pos, radius, color
}

async fn load_painting(path: &str, label: &'static str) -> Painting {
    let texture = load_texture(path).await.expect("failed to load painting texture");
    texture.set_filter(FilterMode::Linear);
    Painting { texture, label }
}

fn room_center_x(room: usize) -> f32 {
    ROOM_WIDTH * room as f32 + ROOM_WIDTH / 2.0
}

fn build_level(paintings: &[Painting; 9], lighting: &Lighting) -> Level {
    let floor_tex = make_floor_texture();
    let ceiling_tex = make_ceiling_texture();

    let mut floor_v = Vec::new();
    let mut floor_i = Vec::new();
    let mut ceiling_v = Vec::new();
    let mut ceiling_i = Vec::new();

    let wall_color = Color::new(1.0, 1.0, 1.0, 1.0);
    let floor_color = Color::new(1.0, 1.0, 1.0, 1.0);
    let ceiling_color = Color::new(1.0, 1.0, 1.0, 1.0);

    // Floor & ceiling span the whole level length as one continuous slab.
    build_quad(
        &mut floor_v,
        &mut floor_i,
        vec3(0.0, 0.0, 0.0),
        vec3(LEVEL_LENGTH, 0.0, 0.0),
        vec3(0.0, 0.0, ROOM_DEPTH),
        vec3(0.0, 1.0, 0.0),
        (24, 10),
        floor_color,
        lighting,
    );
    build_quad(
        &mut ceiling_v,
        &mut ceiling_i,
        vec3(0.0, ROOM_HEIGHT, ROOM_DEPTH),
        vec3(LEVEL_LENGTH, 0.0, 0.0),
        vec3(0.0, 0.0, -ROOM_DEPTH),
        vec3(0.0, -1.0, 0.0),
        (24, 10),
        ceiling_color,
        lighting,
    );

    // Walls are built per room so each room can have its own wall color.
    // Outer walls (z=0, z=ROOM_DEPTH, and the two end caps) are split into
    // each room's x segment; dividing walls between rooms contribute one
    // face (with a doorway gap) to each of the two rooms they separate.
    let mut opaque_meshes = Vec::new();
    for room in 0..NUM_ROOMS {
        let mut wv = Vec::new();
        let mut wi = Vec::new();
        let x0 = ROOM_WIDTH * room as f32;
        let x1 = ROOM_WIDTH * (room + 1) as f32;

        // South wall (z=0).
        build_quad(
            &mut wv,
            &mut wi,
            vec3(x0, 0.0, 0.0),
            vec3(x1 - x0, 0.0, 0.0),
            vec3(0.0, ROOM_HEIGHT, 0.0),
            vec3(0.0, 0.0, 1.0),
            (8, 4),
            wall_color,
            lighting,
        );
        // North wall (z=ROOM_DEPTH), the one paintings hang on.
        build_quad(
            &mut wv,
            &mut wi,
            vec3(x1, 0.0, ROOM_DEPTH),
            vec3(x0 - x1, 0.0, 0.0),
            vec3(0.0, ROOM_HEIGHT, 0.0),
            vec3(0.0, 0.0, -1.0),
            (8, 4),
            wall_color,
            lighting,
        );

        // End caps for the first/last room.
        if room == 0 {
            build_quad(
                &mut wv,
                &mut wi,
                vec3(0.0, 0.0, ROOM_DEPTH),
                vec3(0.0, 0.0, -ROOM_DEPTH),
                vec3(0.0, ROOM_HEIGHT, 0.0),
                vec3(1.0, 0.0, 0.0),
                (10, 4),
                wall_color,
                lighting,
            );
        }
        if room == NUM_ROOMS - 1 {
            build_quad(
                &mut wv,
                &mut wi,
                vec3(LEVEL_LENGTH, 0.0, 0.0),
                vec3(0.0, 0.0, ROOM_DEPTH),
                vec3(0.0, ROOM_HEIGHT, 0.0),
                vec3(-1.0, 0.0, 0.0),
                (10, 4),
                wall_color,
                lighting,
            );
        }

        // Dividing wall on this room's far side (shared with room+1): the
        // face normal points back into this room.
        if room < NUM_ROOMS - 1 {
            let x = x1 - DIVIDER_EPS;
            build_quad(
                &mut wv,
                &mut wi,
                vec3(x, 0.0, 0.0),
                vec3(0.0, 0.0, DOOR_Z_MIN),
                vec3(0.0, ROOM_HEIGHT, 0.0),
                vec3(-1.0, 0.0, 0.0),
                (4, 4),
                wall_color,
                lighting,
            );
            build_quad(
                &mut wv,
                &mut wi,
                vec3(x, 0.0, DOOR_Z_MAX),
                vec3(0.0, 0.0, ROOM_DEPTH - DOOR_Z_MAX),
                vec3(0.0, ROOM_HEIGHT, 0.0),
                vec3(-1.0, 0.0, 0.0),
                (4, 4),
                wall_color,
                lighting,
            );
            build_quad(
                &mut wv,
                &mut wi,
                vec3(x, ROOM_HEIGHT * 0.75, DOOR_Z_MIN),
                vec3(0.0, 0.0, DOOR_Z_MAX - DOOR_Z_MIN),
                vec3(0.0, ROOM_HEIGHT * 0.25, 0.0),
                vec3(-1.0, 0.0, 0.0),
                (2, 1),
                wall_color,
                lighting,
            );
        }
        // Dividing wall on this room's near side (shared with room-1): the
        // face normal points back into this room.
        if room > 0 {
            let x = x0 + DIVIDER_EPS;
            build_quad(
                &mut wv,
                &mut wi,
                vec3(x, 0.0, DOOR_Z_MIN),
                vec3(0.0, 0.0, -DOOR_Z_MIN),
                vec3(0.0, ROOM_HEIGHT, 0.0),
                vec3(1.0, 0.0, 0.0),
                (4, 4),
                wall_color,
                lighting,
            );
            build_quad(
                &mut wv,
                &mut wi,
                vec3(x, 0.0, ROOM_DEPTH),
                vec3(0.0, 0.0, DOOR_Z_MAX - ROOM_DEPTH),
                vec3(0.0, ROOM_HEIGHT, 0.0),
                vec3(1.0, 0.0, 0.0),
                (4, 4),
                wall_color,
                lighting,
            );
            build_quad(
                &mut wv,
                &mut wi,
                vec3(x, ROOM_HEIGHT * 0.75, DOOR_Z_MAX),
                vec3(0.0, 0.0, DOOR_Z_MIN - DOOR_Z_MAX),
                vec3(0.0, ROOM_HEIGHT * 0.25, 0.0),
                vec3(1.0, 0.0, 0.0),
                (2, 1),
                wall_color,
                lighting,
            );
        }

        opaque_meshes.push(Mesh {
            vertices: wv,
            indices: wi,
            texture: Some(make_wall_texture(room_color(room))),
        });
    }

    opaque_meshes.push(Mesh {
        vertices: floor_v,
        indices: floor_i,
        texture: Some(floor_tex),
    });
    opaque_meshes.push(Mesh {
        vertices: ceiling_v,
        indices: ceiling_i,
        texture: Some(ceiling_tex),
    });

    // Paintings: 3 per room, mounted on the far wall (z = ROOM_DEPTH),
    // facing back into the room (-z normal).
    let frame_color = Color::new(0.12, 0.09, 0.07, 1.0);
    let mut painting_meshes = Vec::new();
    let pic_w = 1.8f32;
    let pic_h = 1.3f32;
    let pic_y = 1.9f32;
    let frame_border = 0.12f32;
    let wall_z = ROOM_DEPTH - 0.01; // avoid z-fighting with wall

    for room in 0..NUM_ROOMS {
        let cx = room_center_x(room);
        let spacing = ROOM_WIDTH / 3.0;
        let xs = [cx - spacing, cx, cx + spacing];
        for (slot, &px) in xs.iter().enumerate() {
            let idx = room * 3 + slot;
            let painting = &paintings[idx];

            // Frame (slightly larger, dark quad behind the picture).
            let mut frame_v = Vec::new();
            let mut frame_i = Vec::new();
            build_flat_quad(
                &mut frame_v,
                &mut frame_i,
                vec3(
                    px - pic_w / 2.0 - frame_border,
                    pic_y - pic_h / 2.0 - frame_border,
                    wall_z,
                ),
                vec3(pic_w + frame_border * 2.0, 0.0, 0.0),
                vec3(0.0, pic_h + frame_border * 2.0, 0.0),
                vec3(0.0, 0.0, -1.0),
                frame_color,
                lighting,
            );
            painting_meshes.push(Mesh {
                vertices: frame_v,
                indices: frame_i,
                texture: None,
            });

            // The painting itself (brighter — simulated gallery spotlight).
            let mut pic_v = Vec::new();
            let mut pic_i = Vec::new();
            build_flat_quad(
                &mut pic_v,
                &mut pic_i,
                vec3(px - pic_w / 2.0, pic_y - pic_h / 2.0, wall_z - 0.005),
                vec3(pic_w, 0.0, 0.0),
                vec3(0.0, pic_h, 0.0),
                vec3(0.0, 0.0, -1.0),
                Color::new(1.0, 1.0, 1.0, 1.0),
                lighting,
            );
            painting_meshes.push(Mesh {
                vertices: pic_v,
                indices: pic_i,
                texture: Some(painting.texture.weak_clone()),
            });
        }
    }

    // Light fixture visuals: one warm ceiling light per room + a small
    // spotlight nub above each painting.
    let mut fixtures = Vec::new();
    for room in 0..NUM_ROOMS {
        let cx = room_center_x(room);
        fixtures.push((
            vec3(cx, ROOM_HEIGHT - 0.12, ROOM_DEPTH / 2.0),
            0.22,
            Color::new(1.0, 0.95, 0.8, 1.0),
        ));
    }
    for room in 0..NUM_ROOMS {
        let cx = room_center_x(room);
        let spacing = ROOM_WIDTH / 3.0;
        for &px in &[cx - spacing, cx, cx + spacing] {
            fixtures.push((
                vec3(px, ROOM_HEIGHT - 0.3, ROOM_DEPTH - 1.1),
                0.08,
                Color::new(1.0, 0.98, 0.9, 1.0),
            ));
        }
    }

    Level {
        opaque_meshes,
        painting_meshes,
        fixtures,
    }
}

fn build_lighting() -> Lighting {
    let mut lights = Vec::new();
    for room in 0..NUM_ROOMS {
        let cx = room_center_x(room);
        // Main ceiling light.
        lights.push(PointLight {
            pos: vec3(cx, ROOM_HEIGHT - 0.1, ROOM_DEPTH / 2.0),
            color: vec3(1.0, 0.93, 0.78),
            intensity: 6.0,
        });
        // Spotlights over the paintings on the far wall.
        let spacing = ROOM_WIDTH / 3.0;
        for &px in &[cx - spacing, cx, cx + spacing] {
            lights.push(PointLight {
                pos: vec3(px, ROOM_HEIGHT - 0.3, ROOM_DEPTH - 1.1),
                color: vec3(1.0, 0.97, 0.88),
                intensity: 4.2,
            });
        }
    }
    Lighting {
        ambient: vec3(0.16, 0.16, 0.19),
        lights,
    }
}

// ---------------------------------------------------------------------
// Player / collision
// ---------------------------------------------------------------------

struct Player {
    pos: Vec3, // x,z used; y fixed at EYE_HEIGHT
    yaw: f32,
}

fn blocked(p: Vec3) -> bool {
    if p.x < PLAYER_MARGIN
        || p.x > LEVEL_LENGTH - PLAYER_MARGIN
        || p.z < PLAYER_MARGIN
        || p.z > ROOM_DEPTH - PLAYER_MARGIN
    {
        return true;
    }
    for room in 0..NUM_ROOMS - 1 {
        let wall_x = ROOM_WIDTH * (room + 1) as f32;
        if (p.x - wall_x).abs() < WALL_THICKNESS_COLLIDE
            && !(p.z > DOOR_Z_MIN + 0.05 && p.z < DOOR_Z_MAX - 0.05)
        {
            return true;
        }
    }
    false
}

impl Player {
    fn try_move(&mut self, delta: Vec3) {
        let full = self.pos + delta;
        if !blocked(vec3(full.x, 0.0, full.z)) {
            self.pos = full;
            return;
        }
        let x_only = vec3(full.x, self.pos.y, self.pos.z);
        if !blocked(vec3(x_only.x, 0.0, x_only.z)) {
            self.pos = x_only;
            return;
        }
        let z_only = vec3(self.pos.x, self.pos.y, full.z);
        if !blocked(vec3(z_only.x, 0.0, z_only.z)) {
            self.pos = z_only;
        }
    }
}

// ---------------------------------------------------------------------
// HUD
// ---------------------------------------------------------------------

/// A small instructions label (title + a key-cap diagram of the arrow
/// keys) drawn in screen space over the 3D view.
fn draw_controls_label() {
    let panel_x = 12.0;
    let panel_y = 12.0;
    let panel_w = 250.0;
    let panel_h = 96.0;

    draw_rectangle(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        Color::new(0.0, 0.0, 0.0, 0.55),
    );
    draw_rectangle_lines(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        2.0,
        Color::new(1.0, 1.0, 1.0, 0.25),
    );

    draw_text("GALLERY3D", panel_x + 14.0, panel_y + 26.0, 22.0, WHITE);
    draw_text(
        "move / turn",
        panel_x + 14.0,
        panel_y + 48.0,
        16.0,
        Color::new(1.0, 1.0, 1.0, 0.7),
    );

    let key_size = 24.0;
    let draw_key = |x: f32, y: f32, glyph: &str| {
        draw_rectangle(x, y, key_size, key_size, Color::new(1.0, 1.0, 1.0, 0.15));
        draw_rectangle_lines(x, y, key_size, key_size, 1.5, Color::new(1.0, 1.0, 1.0, 0.65));
        let dims = measure_text(glyph, None, 18, 1.0);
        draw_text(
            glyph,
            x + (key_size - dims.width) / 2.0,
            y + key_size - 6.0,
            18.0,
            WHITE,
        );
    };

    // Arrow-key cluster: Up above, Left/Down/Right in a row below.
    let cluster_x = panel_x + panel_w - 96.0;
    let cluster_y = panel_y + 36.0;
    draw_key(cluster_x + 26.0, cluster_y, "^");
    draw_key(cluster_x, cluster_y + 26.0, "<");
    draw_key(cluster_x + 26.0, cluster_y + 26.0, "v");
    draw_key(cluster_x + 52.0, cluster_y + 26.0, ">");
}

// ---------------------------------------------------------------------
// Touch controls (mobile) — also usable with a held mouse click, which is
// handy for testing without a touchscreen.
// ---------------------------------------------------------------------

struct TouchButton {
    x: f32,
    y: f32,
    size: f32,
    glyph: &'static str,
}

impl TouchButton {
    fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.size && py >= self.y && py <= self.y + self.size
    }
}

struct TouchControls {
    forward: TouchButton,
    back: TouchButton,
    left: TouchButton,
    right: TouchButton,
}

/// Lays out two virtual pads sized for a fingertip: forward/back at
/// bottom-left, turn left/right at bottom-right. Recomputed every frame
/// since the canvas can resize (e.g. a phone rotating).
fn layout_touch_controls() -> TouchControls {
    let sw = screen_width();
    let sh = screen_height();
    let size = (sw * 0.12).clamp(48.0, 72.0);
    let gap = size * 0.18;
    let margin = size * 0.5;

    let move_cx = margin + size * 0.5;
    let move_top = sh - margin - size * 2.0 - gap;
    let forward = TouchButton {
        x: move_cx - size / 2.0,
        y: move_top,
        size,
        glyph: "^",
    };
    let back = TouchButton {
        x: move_cx - size / 2.0,
        y: move_top + size + gap,
        size,
        glyph: "v",
    };

    let turn_y = sh - margin - size;
    let right = TouchButton {
        x: sw - margin - size,
        y: turn_y,
        size,
        glyph: ">",
    };
    let left = TouchButton {
        x: right.x - size - gap,
        y: turn_y,
        size,
        glyph: "<",
    };

    TouchControls {
        forward,
        back,
        left,
        right,
    }
}

/// Every currently-pressed pointer: active touches, plus a held left mouse
/// button (so the same buttons work for a quick test with a mouse).
fn active_pointers() -> Vec<(f32, f32)> {
    let mut points: Vec<(f32, f32)> = touches()
        .iter()
        .filter(|t| !matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled))
        .map(|t| (t.position.x, t.position.y))
        .collect();
    if is_mouse_button_down(MouseButton::Left) {
        let (mx, my) = mouse_position();
        points.push((mx, my));
    }
    points
}

fn draw_touch_button(btn: &TouchButton, active: bool) {
    let bg = if active {
        Color::new(1.0, 1.0, 1.0, 0.38)
    } else {
        Color::new(1.0, 1.0, 1.0, 0.14)
    };
    draw_rectangle(btn.x, btn.y, btn.size, btn.size, bg);
    draw_rectangle_lines(btn.x, btn.y, btn.size, btn.size, 2.0, Color::new(1.0, 1.0, 1.0, 0.5));

    let font_size = (btn.size * 0.5) as u16;
    let dims = measure_text(btn.glyph, None, font_size, 1.0);
    draw_text(
        btn.glyph,
        btn.x + (btn.size - dims.width) / 2.0,
        btn.y + btn.size / 2.0 + dims.height / 2.0,
        font_size as f32,
        WHITE,
    );
}

fn draw_touch_controls(touch: &TouchControls, forward: bool, back: bool, left: bool, right: bool) {
    draw_touch_button(&touch.forward, forward);
    draw_touch_button(&touch.back, back);
    draw_touch_button(&touch.left, left);
    draw_touch_button(&touch.right, right);
}

// ---------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------

fn window_conf() -> Conf {
    Conf {
        window_title: "Gallery3D — a macroquad first-person demo".to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    set_mouse_cursor(CursorIcon::Default);

    let painting_specs: [(&str, &str); 9] = [
        ("assets/images/room1_apples.png", "Shiny Apples"),
        ("assets/images/room1_fruitbowl.png", "Fruit Bowl"),
        ("assets/images/room1_spheres.png", "Two Spheres on a Table"),
        ("assets/images/room2_sponza.png", "Sponza"),
        ("assets/images/room2_helmet.png", "Damaged Helmet"),
        ("assets/images/room2_flighthelmet.png", "Flight Helmet"),
        ("assets/images/room3_cornell.png", "Cornell Box"),
        ("assets/images/room3_glass.png", "Glass Marbles"),
        ("assets/images/room3_neon.png", "Neon"),
    ];

    let mut paintings_vec = Vec::with_capacity(9);
    for (path, label) in painting_specs {
        paintings_vec.push(load_painting(path, label).await);
    }
    let paintings: [Painting; 9] = paintings_vec
        .try_into()
        .unwrap_or_else(|_| panic!("expected exactly 9 paintings"));

    let lighting = build_lighting();
    let level = build_level(&paintings, &lighting);

    // Spawn in the middle of the first room, facing the paintings on the
    // far wall, so the player starts inside the gallery rather than
    // staring at the nearest blank wall.
    let mut player = Player {
        pos: vec3(room_center_x(0), 0.0, ROOM_DEPTH / 2.0),
        yaw: std::f32::consts::PI,
    };

    loop {
        let dt = get_frame_time();

        let touch_controls = layout_touch_controls();
        let pointers = active_pointers();
        let touch_forward = pointers.iter().any(|&(x, y)| touch_controls.forward.contains(x, y));
        let touch_back = pointers.iter().any(|&(x, y)| touch_controls.back.contains(x, y));
        let touch_left = pointers.iter().any(|&(x, y)| touch_controls.left.contains(x, y));
        let touch_right = pointers.iter().any(|&(x, y)| touch_controls.right.contains(x, y));

        if is_key_down(KeyCode::Left) || touch_left {
            player.yaw -= TURN_SPEED * dt;
        }
        if is_key_down(KeyCode::Right) || touch_right {
            player.yaw += TURN_SPEED * dt;
        }
        let forward = vec3(player.yaw.sin(), 0.0, -player.yaw.cos());
        if is_key_down(KeyCode::Up) || touch_forward {
            player.try_move(forward * MOVE_SPEED * dt);
        }
        if is_key_down(KeyCode::Down) || touch_back {
            player.try_move(-forward * MOVE_SPEED * dt);
        }

        let eye = vec3(player.pos.x, EYE_HEIGHT, player.pos.z);
        let target = eye + forward;

        clear_background(Color::new(0.03, 0.03, 0.05, 1.0));

        set_camera(&Camera3D {
            position: eye,
            up: vec3(0.0, 1.0, 0.0),
            target,
            fovy: 1.15,
            ..Default::default()
        });

        for mesh in &level.opaque_meshes {
            draw_mesh(mesh);
        }
        for mesh in &level.painting_meshes {
            draw_mesh(mesh);
        }
        for &(pos, radius, color) in &level.fixtures {
            draw_sphere(pos, radius, None, color);
        }

        set_default_camera();

        draw_controls_label();
        draw_touch_controls(&touch_controls, touch_forward, touch_back, touch_left, touch_right);

        draw_text(
            &format!("FPS: {}", get_fps()),
            16.0,
            128.0,
            20.0,
            Color::new(1.0, 1.0, 1.0, 0.6),
        );

        next_frame().await;
    }
}
