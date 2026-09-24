module Outer = struct
  type color = Red | Blue
  type person = { age : int }
  exception Oops
  let count = 1
  let add x = x + count
  class counter = object method get = count end
end
module type S = sig val run : int -> int end
external primitive : int -> int = "primitive"
