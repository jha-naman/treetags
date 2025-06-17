var func = function() {};
const fn = (foo, bar) => {};
String.prototype.fn = function() {};
function() {
	function inner() {};
}();
var o = {
	fn: () => {},
};

class Rectangle {
  constructor(height, width) {
	this.height = height;
	this.width = width;
  }

  area() {
	return this.height * this.width;
  }
}

