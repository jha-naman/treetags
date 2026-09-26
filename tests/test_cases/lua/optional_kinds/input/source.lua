local cache = 1
local pending
do
  local block_local = 1
  block_local = 2
end
block_local = 3
for i = 1, 2 do
  i = i + 1
end
i = 3
counter = 0
cache = 2
config = {
  title = "demo",
  enabled = true,
  nested = { value = 3 },
  handler = function(x) return x end,
}
config.limit = 10
config["mode"] = "fast"
config[dynamic] = 3
for key, value in pairs(config) do
  key = key
end
function config:refresh(delta)
  delta = delta + 1
  local inside = delta
  inside = inside + 1
  self.limit = inside
  global_in_function = 7
  ::again::
  return inside
end
function config.utility(value) return value end
local function helper(arg)
  local scope_value = arg
  scope_value = 2
  ::done::
  return scope_value
end
local handle = function(x) return x end
