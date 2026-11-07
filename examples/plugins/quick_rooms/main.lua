-- Quick rooms: an example ConstructCraft plugin.
local rect = require("lib.rect")

ccad.command {
  name = "rectangle_room",
  title = "Rectangular room",
  description = "Four walls around a rectangle, its floor and its room, with an optional door "
    .. "centered on one side. Returns the ids it created.",
  params = {
    properties = {
      x = { type = "number", description = "West side, meters (default 0)." },
      y = { type = "number", description = "South side, meters (default 0)." },
      width = { type = "number", description = "Size along x, meters." },
      depth = { type = "number", description = "Size along y, meters." },
      height = { type = "number", description = "Wall height, meters (default 2.8)." },
      level = { description = "Level id or name (default: the lowest level)." },
      name = { type = "string", description = "Room name." },
      door = { type = "string", enum = { "south", "east", "north", "west" }, description = "Side with a door." },
    },
    required = { "width", "depth" },
  },
  run = function(args)
    if args.width <= 0 or args.depth <= 0 then
      error("width and depth must be positive", 0)
    end
    local x, y = args.x or 0, args.y or 0
    local p = rect.corners(x, y, args.width, args.depth)
    local walls = {}
    for i = 1, 4 do
      local wall = ccad.run("wall_create", {
        start = p[i], ["end"] = p[i % 4 + 1], height = args.height or 2.8, level = args.level,
      })
      walls[rect.sides[i]] = wall.id
    end
    local floor = ccad.run("floor_create", { boundary = p, level = args.level })
    local room = ccad.run("room_create", { point = rect.center(x, y, args.width, args.depth), name = args.name, level = args.level })
    local out = { walls = walls, floor = floor.id, room = room.id }
    if args.door then
      out.door = ccad.run("door_create", { wall = walls[args.door] }).id
    end
    return out
  end,
}

ccad.command {
  name = "wall_totals",
  title = "Wall totals",
  description = "Count and total length of the walls of each type (meters).",
  kind = "query",
  run = function()
    local totals = {}
    for _, item in ipairs(ccad.run("elements_list", { category = "wall" })) do
      local wall = ccad.run("element_get", { id = item.id })
      local name = wall.type.name
      local t = totals[name] or { count = 0, length = 0 }
      t.count = t.count + 1
      t.length = t.length + wall.derived.length
      totals[name] = t
    end
    return totals
  end,
}
