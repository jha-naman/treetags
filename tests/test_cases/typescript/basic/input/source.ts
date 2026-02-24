var func = function() {};
let foo = 'bar';
const bar = 'baz';
const fn = (foo, bar) => {};
String.prototype.fn = function() {};
function() {
	function inner() {};
}();
Math.PROP = {
  fn: () => {},
  property: 1,
 };

class Rectangle {
  constructor(height, width) {
	this.height = height;
	this.width = width;
  }

  static area() {
	return this.height * this.width;
  }

  set height(x) {
    this.height = x;
  }

  get height() {
    return this.height;
  }
}

class Fields {field1
  field2 = []
  field3;
  field4 = function(x, y) {
	  return x + y
  }
  ['field5'] = 1

  method() {
	return
  }
}

let numbers: Array<number> = [1];

enum Enum { Variant1, Variant2 };

function main(): void { console.log("we is main"); };
let fun = (i: string | number): string => { return `${i}`; };

interface Point {
		readonly x: number,
		y: number,
		z?: number,
		distance_from_origin(): number,
		useless_function(useless_param: number): void,
}

class APoint implements Point {
		x: number,

		constructor(x: number, public y: number, public z: number) {
				this.x = x;
		}

		distance_from_origin(): number {
				return Math.sqrt(this.x*this.x + this.y*this.y + this.z*this.z);
		}

		useless_function(useless_param: number): void {}
}

var p1: Point = {x: 1, y: 2, z: 3 };

type Foo = 
		| { type: "foo" }
		| { type: "bar" }
		| { type: "baz" };

declare const foo: Foo;

type TemplateLiteralFoo = "foo" | "bar" | "baz";

module Mod {
		export class ModFoo<T1, T2> {}
}

let modFoo = new Mod.Foo();

import M = Mod;

