package demo.sample

class Box(val constant: Int, var variable: Int) {
    fun value() = constant
    val stored = 1
}

interface Named {
    fun name(): String
}

object Singleton {
    val answer = 42
}

typealias Alias = Box

val (first, second) = Pair(1, 2)
var mutable = 1

fun top() {
    val local = 2
    fun nested() {}
}
