//! Interface icons: SVG drawings in `assets/icons/ui` (24 x 24 grid, white
//! strokes and fills), rasterized with resvg at the exact pixel size they
//! are shown at and tinted with theme colors: strokes in one tone, soft
//! fills in a second (see [`Tones`]), and an optional [`Badge`] marks what
//! kind of action a command is. Textures are cached per icon, layer and
//! size, so each one is rasterized once.

use egui::{Color32, ColorImage, Context, Painter, Rect, TextureHandle, TextureOptions};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    New,
    Open,
    Save,
    SaveAs,
    Export,
    About,
    Update,
    Wall,
    Door,
    Window,
    Floor,
    Ceiling,
    Roof,
    Select,
    Measure,
    Group,
    Ungroup,
    SelectInstances,
    Copy,
    Delete,
    Undo,
    Redo,
    FloorPlan,
    CeilingPlan,
    View3D,
    Section,
    Elevation,
    Legend,
    Schedule,
    Sheet,
    ZoomFit,
    ProjectInfo,
    Levels,
    Materials,
    MacroRecord,
    MacroRun,
    Text,
    Leader,
    ChevronDown,
    ChevronUp,
    Search,
    ChevronRight,
    Plus,
    Home,
    ArrowLeft,
    CloseProject,
    Close,
    Room,
    Grid,
    Dimension,
    Column,
    Beam,
    ColumnsAtGrids,
    AlignGridEnds,
    BeamsOnGrids,
    BeamsBetweenColumns,
    Footing,
    FootingsUnderColumns,
    StripFooting,
    FoundationSlab,
    SlabSupports,
    Analytical,
    Truss,
    Brace,
    Rebar,
    RebarView,
    StructuralPlan,
    NumberMarks,
    StructureTags,
    Stair,
    Ramp,
    Railing,
    Component,
    Move,
    Rotate,
    MirrorPick,
    MirrorDraw,
    Array,
    TrimCorner,
    TrimExtend,
    Split,
    Offset,
    Align,
    Pin,
    Unpin,
    Join,
    JoinGeometry,
    UnjoinGeometry,
    Cut,
    Paste,
    MatchType,
    DimLinear,
    RefPlane,
    Connector,
    Finish,
    Extrusion,
    DrawLine,
    DrawRect,
    DrawPolygon,
    DrawPolygonOut,
    DrawCircle,
    DrawEllipse,
    DrawArc3,
    DrawArcCenter,
    DrawArcTangent,
    EditBoundary,
    Revolution,
    Sweep,
    Blend,
    Equal,
    DimOrdinate,
    DimAngular,
    DimRadial,
    DimDiameter,
    Pipe,
    Conduit,
    PlumbingFixture,
    Outlet,
    PlumbingPlan,
    ElectricalPlan,
    Systems,
    Connect,
    Sizing,
    Visibility,
    Filter,
    ViewTemplate,
    Hide,
    Isolate,
    OverrideGraphics,
    ResetTemporary,
    Phases,
    Demolish,
    Image,
    Library,
    Settings,
    Keyboard,
    Interface,
    CloseViews,
    Mass,
    PushPull,
    CadImport,
}

impl Icon {
    #[cfg(test)]
    pub const ALL: [Icon; 141] = [
        Icon::New,
        Icon::Open,
        Icon::Save,
        Icon::SaveAs,
        Icon::Export,
        Icon::About,
        Icon::Update,
        Icon::Wall,
        Icon::Door,
        Icon::Window,
        Icon::Floor,
        Icon::Ceiling,
        Icon::Roof,
        Icon::Select,
        Icon::Measure,
        Icon::Group,
        Icon::Ungroup,
        Icon::SelectInstances,
        Icon::Copy,
        Icon::Delete,
        Icon::Undo,
        Icon::Redo,
        Icon::FloorPlan,
        Icon::CeilingPlan,
        Icon::View3D,
        Icon::Section,
        Icon::Elevation,
        Icon::Legend,
        Icon::Schedule,
        Icon::Sheet,
        Icon::ZoomFit,
        Icon::ProjectInfo,
        Icon::Levels,
        Icon::Materials,
        Icon::MacroRecord,
        Icon::MacroRun,
        Icon::Text,
        Icon::Leader,
        Icon::ChevronDown,
        Icon::ChevronUp,
        Icon::Search,
        Icon::ChevronRight,
        Icon::Plus,
        Icon::Home,
        Icon::ArrowLeft,
        Icon::CloseProject,
        Icon::Close,
        Icon::Room,
        Icon::Grid,
        Icon::Dimension,
        Icon::Column,
        Icon::Beam,
        Icon::ColumnsAtGrids,
        Icon::AlignGridEnds,
        Icon::BeamsOnGrids,
        Icon::BeamsBetweenColumns,
        Icon::Footing,
        Icon::FootingsUnderColumns,
        Icon::StripFooting,
        Icon::FoundationSlab,
        Icon::SlabSupports,
        Icon::Analytical,
        Icon::Truss,
        Icon::Brace,
        Icon::Rebar,
        Icon::RebarView,
        Icon::StructuralPlan,
        Icon::NumberMarks,
        Icon::StructureTags,
        Icon::Stair,
        Icon::Ramp,
        Icon::Railing,
        Icon::Component,
        Icon::Move,
        Icon::Rotate,
        Icon::MirrorPick,
        Icon::MirrorDraw,
        Icon::Array,
        Icon::TrimCorner,
        Icon::TrimExtend,
        Icon::Split,
        Icon::Offset,
        Icon::Align,
        Icon::Pin,
        Icon::Unpin,
        Icon::Join,
        Icon::JoinGeometry,
        Icon::UnjoinGeometry,
        Icon::Cut,
        Icon::Paste,
        Icon::MatchType,
        Icon::DimLinear,
        Icon::RefPlane,
        Icon::Connector,
        Icon::Finish,
        Icon::Extrusion,
        Icon::DrawLine,
        Icon::DrawRect,
        Icon::DrawPolygon,
        Icon::DrawPolygonOut,
        Icon::DrawCircle,
        Icon::DrawEllipse,
        Icon::DrawArc3,
        Icon::DrawArcCenter,
        Icon::DrawArcTangent,
        Icon::EditBoundary,
        Icon::Revolution,
        Icon::Sweep,
        Icon::Blend,
        Icon::Equal,
        Icon::DimOrdinate,
        Icon::DimAngular,
        Icon::DimRadial,
        Icon::DimDiameter,
        Icon::Pipe,
        Icon::Conduit,
        Icon::PlumbingFixture,
        Icon::Outlet,
        Icon::PlumbingPlan,
        Icon::ElectricalPlan,
        Icon::Systems,
        Icon::Connect,
        Icon::Sizing,
        Icon::Visibility,
        Icon::Filter,
        Icon::ViewTemplate,
        Icon::Hide,
        Icon::Isolate,
        Icon::OverrideGraphics,
        Icon::ResetTemporary,
        Icon::Phases,
        Icon::Demolish,
        Icon::Image,
        Icon::Library,
        Icon::Settings,
        Icon::Keyboard,
        Icon::Interface,
        Icon::CloseViews,
        Icon::Mass,
        Icon::PushPull,
        Icon::CadImport,
    ];

    /// The icon's SVG source.
    fn source(self) -> &'static str {
        match self {
            Icon::New => include_str!("../../../assets/icons/ui/new.svg"),
            Icon::Open => include_str!("../../../assets/icons/ui/open.svg"),
            Icon::Save => include_str!("../../../assets/icons/ui/save.svg"),
            Icon::SaveAs => include_str!("../../../assets/icons/ui/save-as.svg"),
            Icon::Export => include_str!("../../../assets/icons/ui/export.svg"),
            Icon::About => include_str!("../../../assets/icons/ui/about.svg"),
            Icon::Update => include_str!("../../../assets/icons/ui/update.svg"),
            Icon::Wall => include_str!("../../../assets/icons/ui/wall.svg"),
            Icon::Door => include_str!("../../../assets/icons/ui/door.svg"),
            Icon::Window => include_str!("../../../assets/icons/ui/window.svg"),
            Icon::Floor => include_str!("../../../assets/icons/ui/floor.svg"),
            Icon::Ceiling => include_str!("../../../assets/icons/ui/ceiling.svg"),
            Icon::Roof => include_str!("../../../assets/icons/ui/roof.svg"),
            Icon::Select => include_str!("../../../assets/icons/ui/select.svg"),
            Icon::Measure => include_str!("../../../assets/icons/ui/measure.svg"),
            Icon::Group => include_str!("../../../assets/icons/ui/group.svg"),
            Icon::Ungroup => include_str!("../../../assets/icons/ui/ungroup.svg"),
            Icon::SelectInstances => include_str!("../../../assets/icons/ui/select-instances.svg"),
            Icon::Copy => include_str!("../../../assets/icons/ui/copy.svg"),
            Icon::Delete => include_str!("../../../assets/icons/ui/delete.svg"),
            Icon::Undo => include_str!("../../../assets/icons/ui/undo.svg"),
            Icon::Redo => include_str!("../../../assets/icons/ui/redo.svg"),
            Icon::FloorPlan => include_str!("../../../assets/icons/ui/floor-plan.svg"),
            Icon::CeilingPlan => include_str!("../../../assets/icons/ui/ceiling-plan.svg"),
            Icon::View3D => include_str!("../../../assets/icons/ui/view-3d.svg"),
            Icon::Section => include_str!("../../../assets/icons/ui/section.svg"),
            Icon::Elevation => include_str!("../../../assets/icons/ui/elevation.svg"),
            Icon::Legend => include_str!("../../../assets/icons/ui/legend.svg"),
            Icon::Schedule => include_str!("../../../assets/icons/ui/schedule.svg"),
            Icon::Sheet => include_str!("../../../assets/icons/ui/sheet.svg"),
            Icon::ZoomFit => include_str!("../../../assets/icons/ui/zoom-fit.svg"),
            Icon::ProjectInfo => include_str!("../../../assets/icons/ui/project-info.svg"),
            Icon::Levels => include_str!("../../../assets/icons/ui/levels.svg"),
            Icon::Materials => include_str!("../../../assets/icons/ui/materials.svg"),
            Icon::MacroRecord => include_str!("../../../assets/icons/ui/macro-record.svg"),
            Icon::MacroRun => include_str!("../../../assets/icons/ui/macro-run.svg"),
            Icon::Text => include_str!("../../../assets/icons/ui/text.svg"),
            Icon::Leader => include_str!("../../../assets/icons/ui/leader.svg"),
            Icon::ChevronDown => include_str!("../../../assets/icons/ui/chevron-down.svg"),
            Icon::ChevronUp => include_str!("../../../assets/icons/ui/chevron-up.svg"),
            Icon::Search => include_str!("../../../assets/icons/ui/search.svg"),
            Icon::ChevronRight => include_str!("../../../assets/icons/ui/chevron-right.svg"),
            Icon::Plus => include_str!("../../../assets/icons/ui/plus.svg"),
            Icon::Home => include_str!("../../../assets/icons/ui/home.svg"),
            Icon::ArrowLeft => include_str!("../../../assets/icons/ui/arrow-left.svg"),
            Icon::CloseProject => include_str!("../../../assets/icons/ui/close-project.svg"),
            Icon::Close => include_str!("../../../assets/icons/ui/close.svg"),
            Icon::Room => include_str!("../../../assets/icons/ui/room.svg"),
            Icon::Grid => include_str!("../../../assets/icons/ui/grid.svg"),
            Icon::Dimension => include_str!("../../../assets/icons/ui/dimension.svg"),
            Icon::Column => include_str!("../../../assets/icons/ui/column.svg"),
            Icon::Beam => include_str!("../../../assets/icons/ui/beam.svg"),
            Icon::ColumnsAtGrids => include_str!("../../../assets/icons/ui/columns-at-grids.svg"),
            Icon::AlignGridEnds => include_str!("../../../assets/icons/ui/align-grid-ends.svg"),
            Icon::BeamsOnGrids => include_str!("../../../assets/icons/ui/beams-on-grids.svg"),
            Icon::BeamsBetweenColumns => include_str!("../../../assets/icons/ui/beams-between-columns.svg"),
            Icon::Footing => include_str!("../../../assets/icons/ui/footing.svg"),
            Icon::FootingsUnderColumns => include_str!("../../../assets/icons/ui/footings-under-columns.svg"),
            Icon::StripFooting => include_str!("../../../assets/icons/ui/strip-footing.svg"),
            Icon::FoundationSlab => include_str!("../../../assets/icons/ui/foundation-slab.svg"),
            Icon::SlabSupports => include_str!("../../../assets/icons/ui/slab-supports.svg"),
            Icon::Analytical => include_str!("../../../assets/icons/ui/analytical.svg"),
            Icon::Truss => include_str!("../../../assets/icons/ui/truss.svg"),
            Icon::Brace => include_str!("../../../assets/icons/ui/brace.svg"),
            Icon::Rebar => include_str!("../../../assets/icons/ui/rebar.svg"),
            Icon::RebarView => include_str!("../../../assets/icons/ui/rebar-view.svg"),
            Icon::StructuralPlan => include_str!("../../../assets/icons/ui/structural-plan.svg"),
            Icon::NumberMarks => include_str!("../../../assets/icons/ui/number-marks.svg"),
            Icon::StructureTags => include_str!("../../../assets/icons/ui/structure-tags.svg"),
            Icon::Stair => include_str!("../../../assets/icons/ui/stair.svg"),
            Icon::Ramp => include_str!("../../../assets/icons/ui/ramp.svg"),
            Icon::Railing => include_str!("../../../assets/icons/ui/railing.svg"),
            Icon::Component => include_str!("../../../assets/icons/ui/component.svg"),
            Icon::Move => include_str!("../../../assets/icons/ui/move.svg"),
            Icon::Rotate => include_str!("../../../assets/icons/ui/rotate.svg"),
            Icon::MirrorPick => include_str!("../../../assets/icons/ui/mirror-pick.svg"),
            Icon::MirrorDraw => include_str!("../../../assets/icons/ui/mirror-draw.svg"),
            Icon::Array => include_str!("../../../assets/icons/ui/array.svg"),
            Icon::TrimCorner => include_str!("../../../assets/icons/ui/trim-corner.svg"),
            Icon::TrimExtend => include_str!("../../../assets/icons/ui/trim-extend.svg"),
            Icon::Split => include_str!("../../../assets/icons/ui/split.svg"),
            Icon::Offset => include_str!("../../../assets/icons/ui/offset.svg"),
            Icon::Align => include_str!("../../../assets/icons/ui/align.svg"),
            Icon::Pin => include_str!("../../../assets/icons/ui/pin.svg"),
            Icon::Unpin => include_str!("../../../assets/icons/ui/unpin.svg"),
            Icon::Join => include_str!("../../../assets/icons/ui/join.svg"),
            Icon::JoinGeometry => include_str!("../../../assets/icons/ui/join-geometry.svg"),
            Icon::UnjoinGeometry => include_str!("../../../assets/icons/ui/unjoin-geometry.svg"),
            Icon::Cut => include_str!("../../../assets/icons/ui/cut.svg"),
            Icon::Paste => include_str!("../../../assets/icons/ui/paste.svg"),
            Icon::MatchType => include_str!("../../../assets/icons/ui/match-type.svg"),
            Icon::DimLinear => include_str!("../../../assets/icons/ui/dim-linear.svg"),
            Icon::RefPlane => include_str!("../../../assets/icons/ui/ref-plane.svg"),
            Icon::Connector => include_str!("../../../assets/icons/ui/connector.svg"),
            Icon::Finish => include_str!("../../../assets/icons/ui/finish.svg"),
            Icon::Extrusion => include_str!("../../../assets/icons/ui/extrusion.svg"),
            Icon::DrawLine => include_str!("../../../assets/icons/ui/draw-line.svg"),
            Icon::DrawRect => include_str!("../../../assets/icons/ui/draw-rect.svg"),
            Icon::DrawPolygon => include_str!("../../../assets/icons/ui/draw-polygon.svg"),
            Icon::DrawPolygonOut => include_str!("../../../assets/icons/ui/draw-polygon-out.svg"),
            Icon::DrawCircle => include_str!("../../../assets/icons/ui/draw-circle.svg"),
            Icon::DrawEllipse => include_str!("../../../assets/icons/ui/draw-ellipse.svg"),
            Icon::DrawArc3 => include_str!("../../../assets/icons/ui/draw-arc3.svg"),
            Icon::DrawArcCenter => include_str!("../../../assets/icons/ui/draw-arc-center.svg"),
            Icon::DrawArcTangent => include_str!("../../../assets/icons/ui/draw-arc-tangent.svg"),
            Icon::EditBoundary => include_str!("../../../assets/icons/ui/edit-boundary.svg"),
            Icon::Revolution => include_str!("../../../assets/icons/ui/revolution.svg"),
            Icon::Sweep => include_str!("../../../assets/icons/ui/sweep.svg"),
            Icon::Blend => include_str!("../../../assets/icons/ui/blend.svg"),
            Icon::Equal => include_str!("../../../assets/icons/ui/equal.svg"),
            Icon::DimOrdinate => include_str!("../../../assets/icons/ui/dim-ordinate.svg"),
            Icon::DimAngular => include_str!("../../../assets/icons/ui/dim-angular.svg"),
            Icon::DimRadial => include_str!("../../../assets/icons/ui/dim-radial.svg"),
            Icon::DimDiameter => include_str!("../../../assets/icons/ui/dim-diameter.svg"),
            Icon::Pipe => include_str!("../../../assets/icons/ui/pipe.svg"),
            Icon::Conduit => include_str!("../../../assets/icons/ui/conduit.svg"),
            Icon::PlumbingFixture => include_str!("../../../assets/icons/ui/plumbing-fixture.svg"),
            Icon::Outlet => include_str!("../../../assets/icons/ui/outlet.svg"),
            Icon::PlumbingPlan => include_str!("../../../assets/icons/ui/plumbing-plan.svg"),
            Icon::ElectricalPlan => include_str!("../../../assets/icons/ui/electrical-plan.svg"),
            Icon::Systems => include_str!("../../../assets/icons/ui/systems.svg"),
            Icon::Connect => include_str!("../../../assets/icons/ui/mep-connect.svg"),
            Icon::Sizing => include_str!("../../../assets/icons/ui/mep-sizing.svg"),
            Icon::Visibility => include_str!("../../../assets/icons/ui/visibility.svg"),
            Icon::Filter => include_str!("../../../assets/icons/ui/filter.svg"),
            Icon::ViewTemplate => include_str!("../../../assets/icons/ui/view-template.svg"),
            Icon::Hide => include_str!("../../../assets/icons/ui/hide.svg"),
            Icon::Isolate => include_str!("../../../assets/icons/ui/isolate.svg"),
            Icon::OverrideGraphics => include_str!("../../../assets/icons/ui/override-graphics.svg"),
            Icon::ResetTemporary => include_str!("../../../assets/icons/ui/reset-temporary.svg"),
            Icon::Phases => include_str!("../../../assets/icons/ui/phases.svg"),
            Icon::Demolish => include_str!("../../../assets/icons/ui/demolish.svg"),
            Icon::Image => include_str!("../../../assets/icons/ui/image.svg"),
            Icon::CadImport => include_str!("../../../assets/icons/ui/cad-import.svg"),
            Icon::Library => include_str!("../../../assets/icons/ui/library.svg"),
            Icon::Settings => include_str!("../../../assets/icons/ui/settings.svg"),
            Icon::Keyboard => include_str!("../../../assets/icons/ui/keyboard.svg"),
            Icon::Interface => include_str!("../../../assets/icons/ui/interface.svg"),
            Icon::CloseViews => include_str!("../../../assets/icons/ui/close-views.svg"),
            Icon::Mass => include_str!("../../../assets/icons/ui/mass.svg"),
            Icon::PushPull => include_str!("../../../assets/icons/ui/push-pull.svg"),
        }
    }
}

/// Rasterized icons by (icon, side in physical pixels); `None` when an
/// icon failed to rasterize, so it is not retried every frame.
#[derive(Clone, Default)]
struct Cache(Arc<std::sync::Mutex<Textures>>);

type Textures = HashMap<(Icon, Layer, u32), Option<TextureHandle>>;

/// The two tones of an icon: its strokes and solid marks (ink) and its soft
/// fills (tint). Single-color icons paint the tint as the ink at 20%.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Layer {
    Ink,
    Tint,
}

/// The SVG of one layer: the drawings are white with soft fills at 20%
/// (`fill="#fff" fill-opacity=".2"`), so the layers split by attribute.
fn layer_source(source: &str, layer: Layer) -> String {
    const SOFT: &str = r##"fill="#fff" fill-opacity=".2""##;
    match layer {
        Layer::Ink => source.replace(SOFT, r#"fill="none""#),
        Layer::Tint => source
            .replace(SOFT, "\u{1}")
            .replace(r##"fill="#fff""##, r#"fill="none""#)
            .replace(r##"stroke="#fff""##, r#"stroke="none""#)
            // Strokes are off above (soft fills inherit the root's); adding
            // `stroke` here would duplicate the attribute where a fill sets it.
            .replace('\u{1}', r##"fill="#fff""##),
    }
}

/// A small mark in the bottom-right corner of an icon telling what kind
/// of action it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Badge {
    /// Creates something new.
    New,
    /// Brings data in from a file.
    Import,
    /// Writes data out to a file.
    Export,
    /// Removes something.
    Remove,
}

/// Colors of a two-tone icon and its badge.
#[derive(Debug, Clone, Copy)]
pub struct Tones {
    pub ink: Color32,
    pub tint: Color32,
    /// Badge background; the badge glyph uses `badge_glyph`.
    pub badge: Color32,
    pub badge_glyph: Color32,
}

impl Tones {
    /// Theme tones of an icon in a given state; badges take their semantic color.
    pub fn of(palette: &crate::theme::Palette, badge: Option<Badge>, enabled: bool, selected: bool) -> Tones {
        let badge_color = match badge {
            Some(Badge::New) => palette.success,
            Some(Badge::Remove) => palette.danger,
            _ => palette.accent,
        };
        let glyph = if palette.dark { palette.app_bg } else { Color32::WHITE };
        match (enabled, selected) {
            (false, _) => {
                let faint = palette.text_faint;
                Tones { ink: faint, tint: faint.gamma_multiply(0.35), badge: faint, badge_glyph: glyph }
            }
            (true, true) => Tones {
                ink: palette.accent_text,
                tint: palette.accent.gamma_multiply(0.45),
                badge: badge_color,
                badge_glyph: glyph,
            },
            (true, false) => Tones {
                ink: palette.icon_ink,
                tint: palette.icon_tint.gamma_multiply(0.4),
                badge: badge_color,
                badge_glyph: glyph,
            },
        }
    }
}

/// Paint `icon` centered in `rect` (the largest square that fits), in one color.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    paint_layer(painter, rect, icon, Layer::Tint, color.gamma_multiply(0.2));
    paint_layer(painter, rect, icon, Layer::Ink, color);
}

/// Paint `icon` in two tones, with an optional badge.
pub fn paint_toned(painter: &Painter, rect: Rect, icon: Icon, tones: Tones, badge: Option<Badge>) {
    paint_layer(painter, rect, icon, Layer::Tint, tones.tint);
    paint_layer(painter, rect, icon, Layer::Ink, tones.ink);
    if let Some(badge) = badge {
        paint_badge(painter, rect, badge, tones);
    }
}

/// A disc in the bottom-right quarter of the icon square, with a glyph.
fn paint_badge(painter: &Painter, rect: Rect, badge: Badge, tones: Tones) {
    let side = rect.width().min(rect.height());
    let r = (side * 0.21).max(3.5);
    let center = egui::pos2(rect.center().x + side / 2.0 - r * 0.8, rect.center().y + side / 2.0 - r * 0.8);
    // A ring of the background separates the badge from the strokes beneath.
    let ring = crate::theme::palette(painter.ctx()).panel;
    painter.circle_filled(center, r + 1.2, ring);
    painter.circle_filled(center, r, tones.badge);
    let g = r * 0.5;
    let stroke = egui::Stroke::new((r * 0.32).max(1.2), tones.badge_glyph);
    let c = center;
    let v = egui::vec2;
    match badge {
        Badge::New => {
            painter.line_segment([c - v(g, 0.0), c + v(g, 0.0)], stroke);
            painter.line_segment([c - v(0.0, g), c + v(0.0, g)], stroke);
        }
        Badge::Remove => {
            painter.line_segment([c - v(g, 0.0), c + v(g, 0.0)], stroke);
        }
        // Arrows: down into the icon (import) or up out of it (export).
        Badge::Import | Badge::Export => {
            let dir = if badge == Badge::Import { 1.0 } else { -1.0 };
            let tip = c + v(0.0, g * dir);
            painter.line_segment([c - v(0.0, g * dir), tip], stroke);
            painter.line_segment([tip, tip + v(-g * 0.75, -g * 0.75 * dir)], stroke);
            painter.line_segment([tip, tip + v(g * 0.75, -g * 0.75 * dir)], stroke);
        }
    }
}

fn paint_layer(painter: &Painter, rect: Rect, icon: Icon, layer: Layer, color: Color32) {
    let ppp = painter.pixels_per_point();
    let side = rect.width().min(rect.height());
    // Whole physical pixels, placed on the pixel grid, keep the strokes crisp.
    let px = (side * ppp).round().clamp(1.0, 512.0);
    let size = px / ppp;
    let min = ((rect.center() - egui::vec2(size, size) / 2.0) * ppp).round() / ppp;
    let target = Rect::from_min_size(min, egui::vec2(size, size));
    let Some(Some(texture)) = texture(painter.ctx(), icon, layer, px as u32) else { return };
    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    painter.image(texture.id(), target, uv, color);
}

/// The cached texture of a layer: `Some(None)` when the layer is empty
/// (icons without soft fills) or failed to rasterize.
fn texture(ctx: &Context, icon: Icon, layer: Layer, px: u32) -> Option<Option<TextureHandle>> {
    let cache = ctx.data_mut(|d| d.get_temp_mut_or_default::<Cache>(egui::Id::new("icon-cache")).clone());
    let mut map = cache.0.lock().ok()?;
    let texture = map.entry((icon, layer, px)).or_insert_with(|| {
        let image = rasterize(icon, layer, px);
        if image.is_none() {
            log::warn!("icon {icon:?} failed to rasterize");
        }
        image
            .filter(|image| image.pixels.iter().any(|p| p.a() > 0))
            .map(|image| ctx.load_texture(format!("icon-{icon:?}-{layer:?}-{px}"), image, TextureOptions::LINEAR))
    });
    Some(texture.clone())
}

/// Render a layer of an icon's SVG into a `px` x `px` image (premultiplied alpha).
fn rasterize(icon: Icon, layer: Layer, px: u32) -> Option<ColorImage> {
    let source = layer_source(icon.source(), layer);
    let tree = resvg::usvg::Tree::from_str(&source, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
    let scale = px as f32 / tree.size().width().max(tree.size().height());
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let side = px as usize;
    Some(ColorImage::from_rgba_premultiplied([side, side], pixmap.data()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_rasterizes() {
        for icon in Icon::ALL {
            let image = rasterize(icon, Layer::Ink, 20).unwrap_or_else(|| panic!("{icon:?} does not rasterize"));
            let inked = image.pixels.iter().filter(|p| p.a() > 0).count();
            assert!(inked > 20, "{icon:?} is (nearly) empty");
            // Strokes stay inside the icon, with a margin.
            let edge = image.pixels.iter().enumerate().filter(|(i, p)| {
                let (x, y) = (i % 20, i / 20);
                p.a() > 128 && (x == 0 || y == 0 || x == 19 || y == 19)
            });
            assert_eq!(edge.count(), 0, "{icon:?} touches the border");
        }
    }

    /// Soft fills rasterize too, also when the filled path already sets its stroke.
    #[test]
    fn every_soft_fill_rasterizes() {
        for icon in Icon::ALL {
            if !icon.source().contains(r#"fill-opacity=".2""#) {
                continue;
            }
            let image = rasterize(icon, Layer::Tint, 20).unwrap_or_else(|| panic!("{icon:?} fill does not rasterize"));
            assert!(image.pixels.iter().any(|p| p.a() > 0), "{icon:?} fill is empty");
        }
    }

    /// `ICON_SHEET=path.png cargo test -p cc_app icon_sheet` writes every
    /// icon at a few sizes, dark on light, for review.
    #[test]
    fn icon_sheet() {
        let Ok(path) = std::env::var("ICON_SHEET") else { return };
        let sizes = [16u32, 20, 24, 32, 48];
        let cell = 56usize;
        let cols = Icon::ALL.len();
        let (w, h) = (cols * cell, sizes.len() * cell);
        let mut rgba = vec![248u8; w * h * 4];
        for (row, &px) in sizes.iter().enumerate() {
            for (col, icon) in Icon::ALL.iter().enumerate() {
                let image = rasterize(*icon, Layer::Ink, px).unwrap();
                for (i, p) in image.pixels.iter().enumerate() {
                    let (x, y) = (col * cell + 4 + i % px as usize, row * cell + 4 + i / px as usize);
                    let a = p.a() as f32 / 255.0;
                    let o = (y * w + x) * 4;
                    for c in 0..3 {
                        rgba[o + c] = (248.0 * (1.0 - a) + 40.0 * a) as u8;
                    }
                }
            }
        }
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    }
}
