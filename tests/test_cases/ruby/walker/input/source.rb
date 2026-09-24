require 'json'
require_relative 'helper'
load 'other.rb'

class Outer < Base
  include Comparable
  prepend Mixin
  extend ExtendMe
  CONST = 1
  attr_reader :name, :age
  attr_writer :status
  attr_accessor :foo
  alias old_name name
  alias_method :legacy, :modern
  def regular(a, b = 1, **kw)
  end
  def self.singleton(x)
  end
  class << self
    def inside
    end
  end
  class Inner; end
end
module Space
  ANSWER = 42
end
Global = 3
