local API = {}
local function local_fn(a, b)
  return a + b
end
function global_fn(x)
  return x
end
function API.make(x)
  return x
end
function API:run(y, ...)
  local function nested(z) return z end
  return nested(y)
end
function API.sub:work(v)
  return v
end
local assigned = function(z) return z end
thing = function(q) return q end
API.field = function(v) return v end
local table_functions = {
  first = function(x) return x end,
  second = function(y)
    return y
  end,
  ["third"] = function(z) return z end,
}
local a, b = function() end, function() end
global_fn(1)
API:run(2)
