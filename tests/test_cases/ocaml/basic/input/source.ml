(** A small library. *)
module Math = struct
  let double x = x * 2
  let apply = fun f x -> f x
  let choose = function Some x -> x | None -> 0
  let ( ++ ) x y = x + y
end
module type Printable = sig val print : unit -> unit end
class counter = object
  val mutable count = 0
  method increment = count <- count + 1
  method value = count
end
external length : string -> int = "string_length"
let use x = Math.double x
