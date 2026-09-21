import java.util.ArrayList;
import static java.lang.Math.*;

public class Class {
		public static void main(String[] args) {
				boolean local = true;
				final int SOME_CONSTANT = 1;
		}

		private static class FooClass {
				private static final List<String> A_LIST = Arrays.asList("ONE", "TWO", "THREE");
		}

		static String stringVar;
		private int intVar;
		static { stringVar = "foo" }

		public Class() {}

		public int getIntVar() {
				return this.intVar;
		}
}

interface FooInterface {
		public String foo();
		public default void bar() {}
}

public class Foo implements FooInterface {
		@Override
		public String foo() { return "foo"; }
}

public class Bar extends Foo {
		@Override
		public void bar() { System.out.println("bar"); }
}

public abstract class Abstract {
		public abstract List<String> getAbstractString();
}

public record Record(int foo, String bar) {}

enum Enum { ONE, TWO, THREE }

@interface Foo {
		String foo();
		String bar() default bar;
}
