function add(a, b = 1; c = 0)
   a + b + c
end

struct Foo
   bar: Float64
   Foo() = new(1.0)
end

Foo(bar: Float64) = Foo(bar)

abstract type Baz end

