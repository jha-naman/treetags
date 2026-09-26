defmodule Demo do
  defstruct [:name]
  defexception [:message]
  @type item :: term()
  @typep hidden :: atom()
  @opaque secret :: binary()
  @callback run(term()) :: term()
  @macrocallback build(term()) :: Macro.t()
  def hello(a, b \\ nil), do: a
  defp hidden(x), do: x
  defmacro make(x), do: x
  defguard valid(x) when is_atom(x)
  defdelegate delegated(x), to: Other
  Record.defrecord :user, name: nil
  test "works" do
    assert true
  end
end

defprotocol Printable do
  def render(data)
end

defimpl Printable, for: Demo do
  def render(data), do: data
end

defmodule Operators do
  defmacro left <~> right, do: {left, right}
end
