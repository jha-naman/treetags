module Outer
const ANSWER = 42
x = 1
abstract type AbstractThing end
primitive type Bits <: Integer 32 end
mutable struct Box{T} <: AbstractThing
    value::T
    count::Int
    label
end
function calc(x::Int, y=1)::Int
    local z = x + y
    z
end
short(x) = x
typed_short(x)::Int = x
macro say(x)
    x
end
module Inner
const FLAG = true
end
end
using Dates
import Base: show, +
