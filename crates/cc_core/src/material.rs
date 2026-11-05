//! Materials: identity data, graphics in views (shading, transparency,
//! surface and cut patterns), render appearance and physical properties.
//! Types refer to materials by name.

use serde::{Deserialize, Serialize};

/// Broad family of a material (filters, defaults and IFC categories).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum MaterialClass {
    #[default]
    Generic,
    Concrete,
    Masonry,
    Mortar,
    Metal,
    Wood,
    Glass,
    Plastic,
    Stone,
    Ceramic,
    Gypsum,
    Insulation,
    Fabric,
    Paint,
    Earth,
    Liquid,
    Gas,
}

impl MaterialClass {
    pub const ALL: [MaterialClass; 17] = [
        MaterialClass::Generic,
        MaterialClass::Concrete,
        MaterialClass::Masonry,
        MaterialClass::Mortar,
        MaterialClass::Metal,
        MaterialClass::Wood,
        MaterialClass::Glass,
        MaterialClass::Plastic,
        MaterialClass::Stone,
        MaterialClass::Ceramic,
        MaterialClass::Gypsum,
        MaterialClass::Insulation,
        MaterialClass::Fabric,
        MaterialClass::Paint,
        MaterialClass::Earth,
        MaterialClass::Liquid,
        MaterialClass::Gas,
    ];

    pub fn key(self) -> &'static str {
        match self {
            MaterialClass::Generic => "matclass-generic",
            MaterialClass::Concrete => "matclass-concrete",
            MaterialClass::Masonry => "matclass-masonry",
            MaterialClass::Mortar => "matclass-mortar",
            MaterialClass::Metal => "matclass-metal",
            MaterialClass::Wood => "matclass-wood",
            MaterialClass::Glass => "matclass-glass",
            MaterialClass::Plastic => "matclass-plastic",
            MaterialClass::Stone => "matclass-stone",
            MaterialClass::Ceramic => "matclass-ceramic",
            MaterialClass::Gypsum => "matclass-gypsum",
            MaterialClass::Insulation => "matclass-insulation",
            MaterialClass::Fabric => "matclass-fabric",
            MaterialClass::Paint => "matclass-paint",
            MaterialClass::Earth => "matclass-earth",
            MaterialClass::Liquid => "matclass-liquid",
            MaterialClass::Gas => "matclass-gas",
        }
    }
}

/// Fill pattern drawn on surfaces (elevations) or cuts (plans, sections).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Pattern {
    #[default]
    None,
    Solid,
    /// Lines at 45°.
    Diagonal,
    /// Lines at 135°.
    DiagonalBack,
    /// Both diagonals.
    Crosshatch,
    Horizontal,
    Vertical,
    Grid,
    /// Running bond courses.
    Brick,
    /// Scattered dots and small triangles.
    Concrete,
    /// Fine dots.
    Sand,
    /// Zig-zag batt insulation.
    Insulation,
}

impl Pattern {
    pub const ALL: [Pattern; 12] = [
        Pattern::None,
        Pattern::Solid,
        Pattern::Diagonal,
        Pattern::DiagonalBack,
        Pattern::Crosshatch,
        Pattern::Horizontal,
        Pattern::Vertical,
        Pattern::Grid,
        Pattern::Brick,
        Pattern::Concrete,
        Pattern::Sand,
        Pattern::Insulation,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Pattern::None => "pattern-none",
            Pattern::Solid => "pattern-solid",
            Pattern::Diagonal => "pattern-diagonal",
            Pattern::DiagonalBack => "pattern-diagonal-back",
            Pattern::Crosshatch => "pattern-crosshatch",
            Pattern::Horizontal => "pattern-horizontal",
            Pattern::Vertical => "pattern-vertical",
            Pattern::Grid => "pattern-grid",
            Pattern::Brick => "pattern-brick",
            Pattern::Concrete => "pattern-concrete",
            Pattern::Sand => "pattern-sand",
            Pattern::Insulation => "pattern-insulation",
        }
    }
}

/// A pattern over an optional background pattern, each with its color.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PatternFill {
    pub foreground: Pattern,
    pub foreground_color: [u8; 3],
    pub background: Pattern,
    pub background_color: [u8; 3],
}

impl Default for PatternFill {
    fn default() -> Self {
        Self {
            foreground: Pattern::None,
            foreground_color: [0, 0, 0],
            background: Pattern::None,
            background_color: [255, 255, 255],
        }
    }
}

impl PatternFill {
    pub fn lines(pattern: Pattern) -> Self {
        Self { foreground: pattern, ..Self::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.foreground == Pattern::None && self.background == Pattern::None
    }
}

/// Descriptive and product data.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialIdentity {
    pub description: String,
    pub class: MaterialClass,
    pub comments: String,
    pub keywords: String,
    pub manufacturer: String,
    pub model: String,
    /// Unit cost in the project currency.
    pub cost: Option<f64>,
    pub url: String,
    pub keynote: String,
    pub mark: String,
}

/// How the material looks in views.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialGraphics {
    /// Shade with the appearance color instead of the material color.
    pub use_appearance: bool,
    /// 0 (opaque) to 1 (invisible).
    pub transparency: f64,
    /// Pattern on faces seen in elevation and 3D.
    pub surface: PatternFill,
    /// Pattern where the material is cut (plans, sections).
    pub cut: PatternFill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Finish {
    #[default]
    Matte,
    Satin,
    Glossy,
    Polished,
}

impl Finish {
    pub const ALL: [Finish; 4] = [Finish::Matte, Finish::Satin, Finish::Glossy, Finish::Polished];

    pub fn key(self) -> &'static str {
        match self {
            Finish::Matte => "finish-matte",
            Finish::Satin => "finish-satin",
            Finish::Glossy => "finish-glossy",
            Finish::Polished => "finish-polished",
        }
    }

    /// Strength of the specular highlight, 0 to 1.
    pub fn shininess(self) -> f64 {
        match self {
            Finish::Matte => 0.0,
            Finish::Satin => 0.25,
            Finish::Glossy => 0.55,
            Finish::Polished => 0.85,
        }
    }
}

/// Render appearance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialAppearance {
    pub color: [u8; 3],
    pub finish: Finish,
    pub description: String,
}

impl Default for MaterialAppearance {
    fn default() -> Self {
        Self { color: [200, 200, 200], finish: Finish::Matte, description: String::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Behavior {
    #[default]
    Isotropic,
    Orthotropic,
}

impl Behavior {
    pub const ALL: [Behavior; 2] = [Behavior::Isotropic, Behavior::Orthotropic];

    pub fn key(self) -> &'static str {
        match self {
            Behavior::Isotropic => "behavior-isotropic",
            Behavior::Orthotropic => "behavior-orthotropic",
        }
    }
}

/// Physical properties (SI units); `None` when unknown.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MaterialPhysical {
    pub behavior: Behavior,
    /// kg/m³.
    pub density: Option<f64>,
    /// W/(m·K).
    pub thermal_conductivity: Option<f64>,
    /// J/(kg·K).
    pub specific_heat: Option<f64>,
    /// 1/K.
    pub thermal_expansion: Option<f64>,
    /// MPa.
    pub youngs_modulus: Option<f64>,
    /// Poisson's ratio.
    pub poisson: Option<f64>,
    /// MPa.
    pub compressive_strength: Option<f64>,
    /// MPa.
    pub tensile_strength: Option<f64>,
}

impl MaterialPhysical {
    /// Values as (field key, value) pairs, in display order.
    pub fn values_mut(&mut self) -> [(&'static str, &mut Option<f64>); 8] {
        [
            ("phys-density", &mut self.density),
            ("phys-conductivity", &mut self.thermal_conductivity),
            ("phys-specific-heat", &mut self.specific_heat),
            ("phys-expansion", &mut self.thermal_expansion),
            ("phys-youngs", &mut self.youngs_modulus),
            ("phys-poisson", &mut self.poisson),
            ("phys-compressive", &mut self.compressive_strength),
            ("phys-tensile", &mut self.tensile_strength),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    /// sRGB shading color.
    pub color: [u8; 3],
    #[serde(default)]
    pub identity: MaterialIdentity,
    #[serde(default)]
    pub graphics: MaterialGraphics,
    #[serde(default)]
    pub appearance: MaterialAppearance,
    #[serde(default)]
    pub physical: MaterialPhysical,
}

impl Material {
    /// A plain material: the appearance takes the shading color.
    pub fn new(name: impl Into<String>, color: [u8; 3]) -> Self {
        Self {
            name: name.into(),
            color,
            identity: Default::default(),
            graphics: Default::default(),
            appearance: MaterialAppearance { color, ..Default::default() },
            physical: Default::default(),
        }
    }

    /// Color used to shade faces in views.
    pub fn shading_color(&self) -> [u8; 3] {
        if self.graphics.use_appearance { self.appearance.color } else { self.color }
    }

    /// Replace non-finite or out-of-range numbers; returns the number of fixes.
    pub fn repair(&mut self) -> usize {
        let mut fixes = 0;
        let t = &mut self.graphics.transparency;
        if !t.is_finite() || !(0.0..=1.0).contains(t) {
            *t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
            fixes += 1;
        }
        if self.identity.cost.is_some_and(|c| !c.is_finite()) {
            self.identity.cost = None;
            fixes += 1;
        }
        for (_, v) in self.physical.values_mut() {
            if v.is_some_and(|x| !x.is_finite()) {
                *v = None;
                fixes += 1;
            }
        }
        fixes
    }
}

/// Template data for a default material: class, cut pattern and physical values.
pub fn template_properties(m: &mut Material, class: MaterialClass) {
    let p = &mut m.physical;
    let mut set = |density: f64, conductivity: f64, heat: f64| {
        p.density = Some(density);
        p.thermal_conductivity = Some(conductivity);
        p.specific_heat = Some(heat);
    };
    let cut = match class {
        MaterialClass::Concrete => {
            set(2400.0, 1.75, 1000.0);
            m.physical.youngs_modulus = Some(25_000.0);
            m.physical.compressive_strength = Some(25.0);
            Pattern::Concrete
        }
        MaterialClass::Masonry => {
            set(1600.0, 0.70, 920.0);
            Pattern::Diagonal
        }
        MaterialClass::Mortar => {
            set(1800.0, 1.15, 1000.0);
            Pattern::Sand
        }
        MaterialClass::Wood => {
            set(600.0, 0.15, 1340.0);
            Pattern::DiagonalBack
        }
        MaterialClass::Glass => {
            set(2500.0, 1.0, 840.0);
            m.graphics.transparency = 0.6;
            Pattern::None
        }
        MaterialClass::Gypsum => {
            set(1000.0, 0.35, 840.0);
            Pattern::Sand
        }
        MaterialClass::Ceramic => {
            set(2000.0, 1.05, 920.0);
            Pattern::Crosshatch
        }
        MaterialClass::Fabric => {
            set(300.0, 0.06, 1300.0);
            Pattern::None
        }
        MaterialClass::Insulation => {
            set(30.0, 0.035, 1030.0);
            Pattern::Insulation
        }
        MaterialClass::Earth => {
            set(1800.0, 1.5, 1000.0);
            Pattern::Sand
        }
        _ => Pattern::None,
    };
    m.identity.class = class;
    m.graphics.cut = PatternFill::lines(cut);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_load_and_repair_fixes_numbers() {
        // A version 3 material has only a name and a color.
        let m: Material = ron::from_str("(name: \"Concreto\", color: (170, 170, 165))").unwrap();
        assert_eq!(m.appearance.color, [200, 200, 200]);
        assert_eq!(m.graphics.transparency, 0.0);
        let mut m = Material::new("X", [1, 2, 3]);
        assert_eq!(m.appearance.color, [1, 2, 3]);
        m.graphics.transparency = f64::NAN;
        m.physical.density = Some(f64::INFINITY);
        m.identity.cost = Some(f64::NAN);
        assert_eq!(m.repair(), 3);
        assert_eq!(m.graphics.transparency, 0.0);
        assert_eq!(m.physical.density, None);
    }

    #[test]
    fn shading_can_follow_the_appearance() {
        let mut m = Material::new("X", [10, 20, 30]);
        m.appearance.color = [200, 0, 0];
        assert_eq!(m.shading_color(), [10, 20, 30]);
        m.graphics.use_appearance = true;
        assert_eq!(m.shading_color(), [200, 0, 0]);
    }
}
