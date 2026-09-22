package example

class Box(val constant: Int, var variable: Int) {
    fun value() = constant
}

interface Named {
    fun name(): String
}

typealias Alias = Box
