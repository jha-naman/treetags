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

  area() {
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

