//! Standalone PS1 shell menu renderer (MEMORY CARD / CD PLAYER).
//! Polygons and sprites come from ps2kit::ps1menu::MenuFrame.

use crate::renderer::{Batch, Blend, DepthAction, Renderer, Vertex};
use glam::{Vec2, Vec4};
use ps2kit::ps1menu::{MenuFrame, MenuPolygon, MenuVertex, Ps1Menu};
use ps2kit::VideoMode;

/// Uploaded texture handles for the PS1 shell menu.
pub struct MenuTextures {
    names: Vec<String>,
}

impl Renderer {
    /// Uploads the shell's located TIM textures once; retain the returned handle.
    pub fn set_ps1_menu_assets(&mut self, menu: &Ps1Menu) -> MenuTextures {
        let mut names = Vec::new();
        for (i, tim) in menu.textures.iter().enumerate() {
            let name = format!("ps1menu_tex_{i}");
            self.upload_texture(&name, tim.width, tim.height, &tim.rgba, false);
            names.push(name);
        }
        MenuTextures { names }
    }

    /// Returns an empty texture set for menus not reconstructed yet.
    pub fn set_ps1_menu_assets_empty(&mut self) -> MenuTextures {
        MenuTextures { names: Vec::new() }
    }

    /// Draws the supplied menu frame into `scene`.
    /// PS1 drawing coordinates are interlaced 640x480.
    /// PAL displays with x=3/y=27 in 512 lines (scenes §1).
    pub fn render_ps1_menu(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        _field: f32,
        frame: &MenuFrame,
        tex: &MenuTextures,
        video: VideoMode,
    ) {
        self.video = video;
        let (xoff, yoff, height) = if video == VideoMode::Pal {
            (3.0, 27.0, 512.0)
        } else {
            (0.0, 0.0, 480.0)
        };
        let ndc = |xy: [f32; 2]| {
            Vec4::new(
                (xy[0] + xoff) / 320.0 - 1.0,
                1.0 - (xy[1] + yoff) * 2.0 / height,
                0.0,
                1.0,
            )
        };
        let vertex = |v: MenuVertex| Vertex {
            pos: ndc(v.xy),
            uv: Vec2::ZERO,
            pad: Vec2::ZERO,
            color: Vec4::new(
                v.colour[0] as f32 / 255.0,
                v.colour[1] as f32 / 255.0,
                v.colour[2] as f32 / 255.0,
                1.0,
            ),
        };

        let mut verts = Vec::new();
        let mut batches = Vec::new();

        // 1. Clear quad
        let clr = frame.clear_colour;
        Self::full_quad(
            &mut verts,
            Vec4::new(
                clr[0] as f32 / 255.0,
                clr[1] as f32 / 255.0,
                clr[2] as f32 / 255.0,
                1.0,
            ),
        );

        // 2. Polygons (shaded discs, button backplates, panels)
        for polygon in &frame.polygons {
            match polygon {
                MenuPolygon::Quad(v) => {
                    verts.extend([
                        vertex(v[0]),
                        vertex(v[1]),
                        vertex(v[2]),
                        vertex(v[1]),
                        vertex(v[2]),
                        vertex(v[3]),
                    ]);
                }
                MenuPolygon::Triangle(v) => {
                    verts.extend(v.iter().copied().map(vertex));
                }
                _ => {}
            }
        }
        batches.push(Batch::new("white", Blend::Opaque, 0..verts.len()));

        // 3. Sprites (title plates, button icons, labels, cursor)
        for sprite in &frame.sprites {
            let Some(name) = tex.names.get(sprite.texture) else { continue };
            let start = verts.len();
            let [w, h] = sprite.size;
            let [cx, cy] = sprite.centre;
            let lo = ndc([cx - w / 2.0, cy + h / 2.0]);
            let hi = ndc([cx + w / 2.0, cy - h / 2.0]);
            let [u0, v0, u1, v1] = sprite.uv_rect;
            let tint = Vec4::new(
                sprite.tint[0] as f32 / 255.0,
                sprite.tint[1] as f32 / 255.0,
                sprite.tint[2] as f32 / 255.0,
                sprite.tint[3] as f32 / 255.0,
            );
            Self::quad(&mut verts, lo.x, lo.y, hi.x, hi.y, u0, v0, u1, v1, tint);
            batches.push(Batch::named(name.clone(), Blend::Alpha, start..verts.len()));
        }

        let fmt = self.format_of("scene");
        self.pass(enc, "scene", true, DepthAction::None, |r, rp| {
            r.draw(rp, &verts, &batches, fmt, false)
        });
    }
}
