library my.lib;

import 'dart:math';

const double pi = 3.14;
int _counter = 0;
final greeting = 'hi';
int a, b;

typedef IntList = List<int>;
typedef Compare<T> = int Function(T a, T b);

int add(int a, int b) => a + b;

Future<void> main() async {
  var local = 1;
  void nested() {}
}

int get topGetter => 42;
set topSetter(int v) {}

abstract class Animal<T> extends Base with Walk, Run implements Comparable<T> {
  static const int legs = 4;
  final String name;
  int _age = 0;
  late double weight;

  Animal(this.name, {this.weight = 0});
  Animal.baby(this.name) : _age = 0;
  factory Animal.create() => Animal('x');
  factory Animal.redirect() = Animal.baby;
  external int extMethod();

  int get age => _age;
  set age(int v) => _age = v;

  void speak() {}
  static Animal clone(Animal a) => a;
  T echo<T>(T x) => x;

  Animal operator +(Animal other) => this;
}

mixin Swimmer on Animal implements Comparable {
  void swim() {}
  bool get canSwim => true;
}

enum Color {
  red,
  green,
  blue;

  const Color();
  bool get isRed => this == Color.red;
  void show() {}
}

extension StringExt on String {
  bool get isBlank => trim().isEmpty;
  String repeat(int n) => this * n;
}

extension on int {
  int get doubled => this * 2;
}

class Bar = Animal with Swimmer;

class _Private {}

extension type Meters(int value) {
  int get squared => value * value;
  Meters operator +(Meters o) => Meters(value + o.value);
}

extension type const Id<T>._(String raw) implements String {
  factory Id.gen() => Id._('x');
  void show() {}
}
