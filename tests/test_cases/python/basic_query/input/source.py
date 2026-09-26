from . import Blah as Dee
from A.B import C

class Foo:
	def __init__(self, bar):
		self.bar = bar

	def bar(self):
        nested_func = lambda x:x
		pass

variable: List[int] = [1, 2]
concurrent, Foo.assignment = 1, 2

def func(x: int, y: float) -> float:
	sum = x + y
    sum

async def func(): pass

@func_decorator
def decorated_func():
	pass

@class_decorator
class __Foo1(Bar, Baz):
    def _bar(self,
            baz=None):
        pass
