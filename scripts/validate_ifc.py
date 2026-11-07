"""Validate an IFC file exported by ConstructCraft with IfcOpenShell.

Used in CI only: IfcOpenShell is an independent, open-source reference, never
a build or runtime dependency of ConstructCraft.

Checks:
  * schema and EXPRESS rule validation;
  * every building element is contained in a storey and has a type;
  * every building element's geometry can be built, with openings cut;
  * built volumes match the exported base quantities;
  * spaces are aggregated into a storey and their volume is area x height;
  * distribution systems and circuits group elements contained in a storey;
  * construction phase statuses (Pset_*Common.Status) are IFC4 enumeration
    values, one per element.

Usage: python scripts/validate_ifc.py model.ifc
"""

import sys

import ifcopenshell
import ifcopenshell.geom
import ifcopenshell.util.element
import ifcopenshell.util.shape
import ifcopenshell.validate

ELEMENTS = (
    "IfcWall", "IfcDoor", "IfcWindow", "IfcSlab", "IfcCovering", "IfcRoof",
    "IfcColumn", "IfcBeam", "IfcStair", "IfcRailing",
    "IfcFooting", "IfcReinforcingBar", "IfcFurniture", "IfcSanitaryTerminal", "IfcLightFixture", "IfcBuildingElementProxy",
    "IfcPipeSegment", "IfcStairFlight", "IfcMember",
    "IfcCableCarrierSegment", "IfcPipeFitting", "IfcCableCarrierFitting", "IfcOutlet", "IfcSwitchingDevice", "IfcElectricDistributionBoard",
)
REL_TOL = 1e-3
STATUSES = {"NEW", "EXISTING", "DEMOLISH", "TEMPORARY", "OTHER", "NOTKNOWN", "UNSET"}


def quantity(element, name):
    for qto in ifcopenshell.util.element.get_psets(element, qtos_only=True).values():
        if name in qto:
            return qto[name]
    return None


def layer_thickness(element):
    material = ifcopenshell.util.element.get_material(element, should_skip_usage=True)
    if material is None or not material.is_a("IfcMaterialLayerSet"):
        return None
    return sum(layer.LayerThickness for layer in material.MaterialLayers)


def main(path):
    errors = []
    model = ifcopenshell.open(path)
    print(f"{path}: schema {model.schema}, {len(list(model))} instances")

    logger = ifcopenshell.validate.json_logger()
    ifcopenshell.validate.validate(model, logger, express_rules=True)
    for statement in logger.statements:
        errors.append(f"validation: {statement.get('message')} ({statement.get('instance')})")

    settings = ifcopenshell.geom.settings()
    # Tessellate curved profiles finely enough for the volume comparison.
    settings.set("mesher-angular-deflection", 0.02)
    settings.set("mesher-linear-deflection", 0.0001)
    for entity in ELEMENTS:
        elements = model.by_type(entity)
        print(f"  {entity}: {len(elements)}")
        for element in elements:
            label = f"{element.is_a()} {element.GlobalId} '{element.Name}'"
            if ifcopenshell.util.element.get_container(element) is None:
                errors.append(f"{label}: not contained in a spatial structure")
            if ifcopenshell.util.element.get_type(element) is None:
                errors.append(f"{label}: has no type")
            try:
                shape = ifcopenshell.geom.create_shape(settings, element)
            except Exception as exc:  # noqa: BLE001 - report any kernel failure
                errors.append(f"{label}: geometry failed: {exc}")
                continue
            volume = ifcopenshell.util.shape.get_volume(shape.geometry)
            if volume <= 0:
                errors.append(f"{label}: empty geometry")
                continue

            expected = quantity(element, "NetVolume")
            if expected is None and element.is_a("IfcRoof"):
                gross, thickness = quantity(element, "GrossArea"), layer_thickness(element)
                if gross is not None and thickness is not None:
                    expected = gross * thickness
            if expected is not None and abs(volume - expected) > REL_TOL * max(expected, 1e-9):
                errors.append(f"{label}: volume {volume:.6f} differs from quantity {expected:.6f}")

    spaces = model.by_type("IfcSpace")
    print(f"  IfcSpace: {len(spaces)}")
    for space in spaces:
        label = f"IfcSpace {space.GlobalId} '{space.Name}'"
        parent = ifcopenshell.util.element.get_aggregate(space)
        if parent is None or not parent.is_a("IfcBuildingStorey"):
            errors.append(f"{label}: not aggregated into a storey")
        try:
            shape = ifcopenshell.geom.create_shape(settings, space)
        except Exception as exc:  # noqa: BLE001 - report any kernel failure
            errors.append(f"{label}: geometry failed: {exc}")
            continue
        volume = ifcopenshell.util.shape.get_volume(shape.geometry)
        area, height = quantity(space, "NetFloorArea"), quantity(space, "Height")
        if area is not None and height is not None and abs(volume - area * height) > REL_TOL * area * height:
            errors.append(f"{label}: volume {volume:.6f} differs from area x height {area * height:.6f}")

    systems = model.by_type("IfcDistributionSystem")
    print(f"  IfcDistributionSystem: {len(systems)}")
    for system in systems:
        label = f"{system.is_a()} {system.GlobalId} '{system.Name}'"
        members = [m for rel in system.IsGroupedBy for m in rel.RelatedObjects]
        if not members:
            errors.append(f"{label}: groups no elements")
        for member in members:
            if ifcopenshell.util.element.get_container(member) is None:
                errors.append(f"{label}: member {member.GlobalId} is not in a storey")

    statuses = 0
    for element in model.by_type("IfcElement"):
        found = [
            pset["Status"]
            for name, pset in ifcopenshell.util.element.get_psets(element, psets_only=True).items()
            if name.startswith("Pset_") and name.endswith("Common") and "Status" in pset
        ]
        statuses += len(found)
        label = f"{element.is_a()} {element.GlobalId} '{element.Name}'"
        if len(found) > 1:
            errors.append(f"{label}: {len(found)} statuses")
        for status in found:
            values = status if isinstance(status, (list, tuple)) else [status]
            if not values or any(v not in STATUSES for v in values):
                errors.append(f"{label}: status {status!r} is not an IFC4 element status")
    print(f"  Status: {statuses}")

    for error in errors:
        print(f"ERROR {error}")
    print("OK" if not errors else f"{len(errors)} error(s)")
    return 1 if errors else 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    sys.exit(main(sys.argv[1]))
