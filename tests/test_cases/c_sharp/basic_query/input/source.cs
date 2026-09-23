public void Function() {}
namespace Tests {
	public class TestClass {
		TestClass() {}
		public static void Foo() {}
		public record Record(string Name) {
			public int RecordField;
			public void RecordMethod() {}
		}
		public void Foobar() {
				int local_var = 1;
		}
		public static int count = 0;
		public enum Enum {
			EnumEntity,
			AnotherEnumEntity,
		}
		interface IInterface {
			void Foo();
		}
		public static int IntMember { get; set; }
		public delegate int DelegateTest();
		public static event DelegateTest TestEvent;
	}
}
namespace Tests.Qualified {}

