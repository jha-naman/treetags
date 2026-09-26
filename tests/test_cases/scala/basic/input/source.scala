package demo
package nested

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

trait Named {
  val left, right: Int
  def name: String
}

class Employee(val name: String) extends Named {
  val age: Int = 1
  var active = true
  private def greet(other: String): String = {
    val local = other
    local
  }
}

object Registry {
  type Id = String
  opaque type Token = String
  val (first, second) = (1, 2)
  def make(name: String) = new Employee(name)
}

package object helpers {
  val version = 1
}

enum Color {
  case Red, Blue
  case Rgb(r: Int, g: Int, b: Int)
}

def top(value: Int): Int = value
given defaultName: String = "guest"
