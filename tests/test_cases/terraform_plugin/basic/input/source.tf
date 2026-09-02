variable "foo" {
  description = "the venerable foo variable"
  type = bool
  default = true
}

variable "typed_foo" {
  type = object({
    object: object({
      foo: bool
    })
  })

  default = {
    object = { foo: true }
  }
}

data "some-bucket-name" "bucket-name" {
  name = "bucket-name"
}

locals {
  foo_val = var.foo ? "yes" : "no"
}

module "foo_module" {
  source = "modules/foo"
}

output "foo_val" {
  value = var.foo
}

provider "foo_provider" {
  speciality = "foo is the special one"
}

resource "venerable_resource" "foo" {
  name = var.venerable_resource_name
}
