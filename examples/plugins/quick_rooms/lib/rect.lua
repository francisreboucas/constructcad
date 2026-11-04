-- Corners and sides of an axis-aligned rectangle.
local M = {}

function M.corners(x, y, width, depth)
  return { { x, y }, { x + width, y }, { x + width, y + depth }, { x, y + depth } }
end

-- The sides counter-clockwise from the bottom one, by compass name (y is north).
M.sides = { "south", "east", "north", "west" }

function M.center(x, y, width, depth)
  return { x + width / 2, y + depth / 2 }
end

return M
