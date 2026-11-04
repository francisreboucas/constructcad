-- A small house built by a Lua script (see docs/automation.md, "Lua scripts").
-- ccad --save-as house.ccad lua examples/small_house.lua width=8 depth=6
local w = ccad.args.width or 8
local d = ccad.args.depth or 6
local h = 2.8

ccad.run("project_info_set", { name = "Small house", author = "ccad" })
local corners = { { 0, 0 }, { w, 0 }, { w, d }, { 0, d } }
local outer = {}
for i = 1, 4 do
  outer[i] = ccad.run("wall_create", { start = corners[i], ["end"] = corners[i % 4 + 1], height = h }).id
end
-- An inner wall a little past the middle, with a door near its north end.
local split = w * 0.55
local inner = ccad.run("wall_create", { start = { split, 0 }, ["end"] = { split, d }, height = h }).id
ccad.run("door_create", { wall = outer[1], offset = w / 4 })
ccad.run("door_create", { wall = inner, offset = d * 0.75 })
-- Windows: one on the south wall, east of the door, and one centered on each other outer wall.
ccad.run("window_create", { wall = outer[1], offset = w * 0.8 })
for i = 2, 4 do
  ccad.run("window_create", { wall = outer[i] })
end
ccad.run("floor_create", { from_walls = true })
ccad.run("roof_create", { from_walls = true, overhang = 0.6 })
ccad.run("room_create", { point = { split / 2, d / 2 }, name = "Living room" })
ccad.run("room_create", { point = { (split + w) / 2, d / 2 }, name = "Bedroom" })

local rooms = ccad.run("elements_list", { category = "room" })
print(("%d walls, %d rooms"):format(#ccad.run("elements_list", { category = "wall" }), #rooms))
return { rooms = #rooms }
