//! Schedule definitions: which fields a schedule shows, how rows are sorted
//! and filtered, and whether totals are shown. The values are computed by
//! `cc_views::schedule`.

use crate::Category;
use serde::{Deserialize, Serialize};

/// A column a schedule can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ScheduleField {
    Mark,
    Type,
    Level,
    Comments,
    TypeMark,
    Description,
    Manufacturer,
    Model,
    FireRating,
    Cost,
    Length,
    Height,
    Width,
    Thickness,
    Area,
    Perimeter,
    Volume,
    Sill,
    Host,
    Function,
    Structural,
    Number,
    Name,
    Risers,
    RiserHeight,
    Run,
    Depth,
    ComponentName,
    ComponentKind,
    /// Section size of a column or beam.
    Section,
    /// Length of a beam's body after its joins.
    CutLength,
    /// Main material of the type.
    Material,
    /// Bar diameter of reinforcement.
    Diameter,
    /// Number of bars in a reinforcement set.
    BarCount,
    /// Length of one bar.
    BarLength,
    /// Steel weight, kilograms.
    Weight,
    /// Plumbing system or electrical circuits.
    System,
    /// Fall of a pipe per unit of length.
    Slope,
    /// Apparent power of an electrical device, VA.
    Power,
    /// Drainage fixture units of a plumbing fixture.
    DrainUnits,
    /// The phase the element is built in.
    PhaseCreated,
    /// The phase the element is demolished in.
    PhaseDemolished,
    /// Gross floor area of a mass: the sum of its mass floors.
    FloorArea,
    /// Number of tiles on a roof.
    TileCount,
    /// Total length of a roof's framing members.
    FramingLength,
    /// Volume of a roof's framing members.
    FramingVolume,
    /// How many mass floors a mass has.
    FloorCount,
}

impl ScheduleField {
    /// Translation key of the column header.
    pub fn key(self) -> &'static str {
        match self {
            ScheduleField::Mark => "col-mark",
            ScheduleField::Type => "col-type",
            ScheduleField::Level => "col-level",
            ScheduleField::Comments => "col-comments",
            ScheduleField::TypeMark => "info-type-mark",
            ScheduleField::Description => "info-description",
            ScheduleField::Manufacturer => "info-manufacturer",
            ScheduleField::Model => "info-model",
            ScheduleField::FireRating => "info-fire-rating",
            ScheduleField::Cost => "info-cost",
            ScheduleField::Length => "col-length",
            ScheduleField::Height => "col-height",
            ScheduleField::Width => "col-width",
            ScheduleField::Thickness => "col-thickness",
            ScheduleField::Area => "col-area",
            ScheduleField::Perimeter => "col-perimeter",
            ScheduleField::Volume => "col-volume",
            ScheduleField::Sill => "col-sill",
            ScheduleField::Host => "col-host",
            ScheduleField::Function => "prop-function",
            ScheduleField::Structural => "prop-structural",
            ScheduleField::Number => "col-number",
            ScheduleField::Name => "col-name",
            ScheduleField::Risers => "col-risers",
            ScheduleField::RiserHeight => "col-riser-height",
            ScheduleField::Run => "col-run",
            ScheduleField::Depth => "col-depth",
            ScheduleField::ComponentName => "col-component",
            ScheduleField::ComponentKind => "col-component-kind",
            ScheduleField::Section => "col-section",
            ScheduleField::CutLength => "col-cut-length",
            ScheduleField::Material => "col-material",
            ScheduleField::Diameter => "col-diameter",
            ScheduleField::BarCount => "col-bar-count",
            ScheduleField::BarLength => "col-bar-length",
            ScheduleField::Weight => "col-weight",
            ScheduleField::System => "col-system",
            ScheduleField::Slope => "col-slope",
            ScheduleField::Power => "col-power",
            ScheduleField::DrainUnits => "col-drain-units",
            ScheduleField::PhaseCreated => "col-phase-created",
            ScheduleField::PhaseDemolished => "col-phase-demolished",
            ScheduleField::FloorArea => "col-floor-area",
            ScheduleField::FloorCount => "col-floor-count",
            ScheduleField::TileCount => "col-tile-count",
            ScheduleField::FramingLength => "col-framing-length",
            ScheduleField::FramingVolume => "col-framing-volume",
        }
    }

    /// Fields that make sense for `category`, in a sensible order.
    pub fn available(category: Category) -> Vec<ScheduleField> {
        use ScheduleField as F;
        let common = [F::Mark, F::Type, F::Level, F::Comments];
        let type_info = [F::TypeMark, F::Description, F::Manufacturer, F::Model, F::FireRating, F::Cost];
        let phases = [F::PhaseCreated, F::PhaseDemolished];
        let specific: &[F] = match category {
            Category::Wall => &[F::Length, F::Height, F::Thickness, F::Area, F::Volume, F::Function, F::Structural],
            Category::Door => &[F::Width, F::Height, F::Sill, F::Host, F::ComponentName],
            Category::Window => &[F::Width, F::Height, F::Sill, F::Host, F::ComponentName],
            Category::Floor | Category::Ceiling => &[F::Thickness, F::Area, F::Perimeter, F::Volume],
            Category::Roof => &[
                F::Thickness,
                F::Area,
                F::Perimeter,
                F::Volume,
                F::Slope,
                F::TileCount,
                F::Weight,
                F::FramingLength,
                F::FramingVolume,
            ],
            Category::Column => &[F::Section, F::Height, F::Volume, F::Material, F::Structural],
            Category::Beam => &[F::Section, F::Length, F::CutLength, F::Volume, F::Material],
            Category::Foundation => &[F::Section, F::Length, F::Area, F::Thickness, F::Volume, F::Material],
            Category::Rebar => &[F::Host, F::Diameter, F::BarCount, F::BarLength, F::Length, F::Weight, F::Material],
            Category::Stair => &[F::Width, F::Risers, F::RiserHeight, F::Run, F::Volume],
            Category::Ramp => &[F::Width, F::Slope, F::Run, F::Volume],
            Category::Railing => &[F::Length, F::Height],
            Category::Sweep => &[F::Length, F::Volume],
            Category::Pipe => &[F::Diameter, F::Length, F::System, F::Slope, F::Material],
            Category::Conduit => &[F::Diameter, F::Length, F::System, F::Material],
            Category::Component => &[
                F::ComponentName,
                F::ComponentKind,
                F::Width,
                F::Depth,
                F::Height,
                F::Volume,
                F::System,
                F::Power,
                F::DrainUnits,
            ],
            Category::Mass => &[F::Height, F::FloorCount, F::FloorArea, F::Volume],
            Category::Room => {
                return vec![
                    F::Number,
                    F::Name,
                    F::Level,
                    F::Area,
                    F::Perimeter,
                    F::Height,
                    F::Volume,
                    F::Comments,
                    F::PhaseCreated,
                    F::PhaseDemolished,
                ];
            }
            Category::Grid | Category::Dimension | Category::Text | Category::CadImport => return Vec::new(),
        };
        common.iter().chain(specific).chain(&type_info).chain(&phases).copied().collect()
    }

    /// Whether elements of `category` can have a value for this field: the
    /// fields [`available`](Self::available) to its schedules, plus the type,
    /// level, main material and volume, which most elements have. A filter
    /// rule on a field that applies to none of its categories never matches.
    pub fn applies_to(self, category: Category) -> bool {
        use ScheduleField as F;
        matches!(self, F::Type | F::Level | F::Material | F::Volume) || Self::available(category).contains(&self)
    }

    /// Fields shown by a new schedule of `category`.
    pub fn defaults(category: Category) -> Vec<ScheduleField> {
        use ScheduleField as F;
        match category {
            Category::Wall => vec![F::Mark, F::Type, F::Level, F::Length, F::Height, F::Thickness, F::Area, F::Volume],
            Category::Door => vec![F::Mark, F::Type, F::Width, F::Height, F::Level, F::Host, F::Comments],
            Category::Window => vec![F::Mark, F::Type, F::Width, F::Height, F::Sill, F::Level, F::Host, F::Comments],
            Category::Floor | Category::Ceiling | Category::Roof => {
                vec![F::Mark, F::Type, F::Level, F::Area, F::Perimeter, F::Volume]
            }
            Category::Column => vec![F::Mark, F::Type, F::Level, F::Section, F::Height, F::Volume, F::Material],
            Category::Beam => {
                vec![F::Mark, F::Type, F::Level, F::Section, F::Length, F::CutLength, F::Volume, F::Material]
            }
            Category::Foundation => vec![F::Mark, F::Type, F::Level, F::Section, F::Thickness, F::Volume, F::Material],
            Category::Rebar => vec![F::Host, F::Type, F::Diameter, F::BarCount, F::BarLength, F::Length, F::Weight],
            Category::Stair => vec![F::Mark, F::Type, F::Level, F::Risers, F::RiserHeight, F::Run, F::Volume],
            Category::Ramp => vec![F::Mark, F::Type, F::Level, F::Slope, F::Run, F::Volume],
            Category::Railing => vec![F::Mark, F::Type, F::Level, F::Length],
            Category::Sweep => vec![F::Mark, F::Type, F::Level, F::Length, F::Volume],
            Category::Pipe => vec![F::Type, F::System, F::Level, F::Diameter, F::Length, F::Slope],
            Category::Conduit => vec![F::Type, F::System, F::Level, F::Diameter, F::Length],
            Category::Component => vec![F::Mark, F::ComponentName, F::Type, F::Level, F::Width, F::Depth, F::Height],
            Category::Room => vec![F::Number, F::Name, F::Level, F::Area, F::Perimeter],
            Category::Mass => vec![F::Mark, F::Type, F::Level, F::Height, F::FloorCount, F::FloorArea, F::Volume],
            Category::Grid | Category::Dimension | Category::Text | Category::CadImport => Vec::new(),
        }
    }
}

/// What a schedule shows. Empty `fields` means the category defaults (files
/// written before schedules were configurable).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduleDef {
    pub fields: Vec<ScheduleField>,
    /// Sort rows by this field (the default order is by mark or number).
    pub sort_by: Option<ScheduleField>,
    pub descending: bool,
    /// Keep only rows whose value of the field contains the text (case-insensitive).
    pub filter: Option<(ScheduleField, String)>,
    /// Hide the totals row.
    pub hide_totals: bool,
}

impl ScheduleDef {
    /// The fields to show for `category`.
    pub fn fields_for(&self, category: Category) -> Vec<ScheduleField> {
        if self.fields.is_empty() { ScheduleField::defaults(category) } else { self.fields.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_available() {
        for c in Category::SCHEDULABLE {
            let available = ScheduleField::available(c);
            for f in ScheduleField::defaults(c) {
                assert!(available.contains(&f), "{c:?} default {f:?} not available");
            }
        }
    }
}
