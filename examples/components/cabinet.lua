-- A cabinet drawn by a script piece (see docs/automation.md, "Scripted components").
-- Load it with:
--   ccad run component_create name=Cabinet 'def={"items": [{"name": "Script", "shape": {"Script": {"source": ""}}}]}'
--   ccad run component_script_set type=Cabinet piece=Script source=@examples/components/cabinet.lua
-- or paste it into a script piece in the component editor.

-- Shared by every cabinet of a type.
param.length   { "depth", label = "Depth", default = 0.55 }
param.length   { "height", label = "Height", default = 0.85 }
param.choice   { "handle", { { "bar", "Bar" }, { "knob", "Knob" }, { "none", "None" } }, label = "Handle" }

-- Each cabinet's own.
param.length   { "width", label = "Width", default = 0.8, instance = true }
param.number   { "doors", label = "Doors", default = 2, instance = true }
param.boolean  { "worktop", label = "Worktop", default = true, instance = true }
param.material { "body", label = "Body", default = "", instance = true }

-- Dragged in plans: how far the plinth is set back.
hotspot { "plinth", 0, 0.05, 0, label = "Plinth", shape = "diamond", color = "#ff8800" }

local THICK = 0.018
local PLINTH = 0.1

function geometry(p)
  local w, d, h = p.width, p.depth, p.height
  local back = math.max(0, math.min(p.plinth.y, d / 2))
  local doors = math.max(1, math.floor(p.doors + 0.5))
  local pieces = {
    box { 0, back, 0, w, d - back, PLINTH, name = "plinth", detail = { "medium", "fine" } },
    box { 0, 0, PLINTH, THICK, d, h - PLINTH, name = "left side", material = p.body },
    box { w - THICK, 0, PLINTH, THICK, d, h - PLINTH, name = "right side", material = p.body },
    box { THICK, 0, PLINTH, w - 2 * THICK, THICK, h - PLINTH, name = "back", material = p.body },
  }
  -- The doors, side by side across the front, each with its handle.
  local door = (w - THICK * (doors + 1)) / doors
  for i = 0, doors - 1 do
    local x = THICK + i * (door + THICK)
    pieces[#pieces + 1] = box { x, d - THICK, PLINTH, door, THICK, h - PLINTH - THICK, name = "door " .. (i + 1) }
    local hx = (i % 2 == 0) and (x + door - 0.04) or (x + 0.04)
    if p.handle == "bar" then
      pieces[#pieces + 1] = box { hx - 0.006, d, h - 0.25, 0.012, 0.025, 0.15, name = "handle", detail = { "fine" } }
    elseif p.handle == "knob" then
      pieces[#pieces + 1] = cylinder { hx, d + 0.012, h - 0.12, 0.012, 0.02, name = "knob", detail = { "fine" } }
    end
  end
  if p.worktop then
    pieces[#pieces + 1] = box { 0, 0, h, w, d + 0.02, 0.03, name = "worktop" }
  end
  return pieces
end
