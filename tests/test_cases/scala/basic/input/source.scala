val x = 10
val a: Double = 1.0

def add(x: Int, y: Int): Int = {
  val y2 = x + y
  y2
}

def add2(x: Int, y: Int = 2) = x + y

def add3(x: Int): Int = {
  val anonFunc: Int => Int = { z =>
    z + 3
  }
  anonFunc(x)
}

val (a, b) = (1, 2)

class Foo(a: String) {
  var b: String = a
  private def c = "see"
}

object Bar {
  def baz = 1
}

case class Person(first_name: String, last_name: String)

trait Trait {
  def isCool: Boolean
}

