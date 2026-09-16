const std = @import("std");
const InternalCount: usize = 4;
pub var active: bool = true;

pub const Point = struct {
    x: f64,
    y: f64 = 0,
    const origin_name = "zero";

    pub fn distance(self: Point, other: Point) f64 {
        const dx = self.x - other.x;
        return dx;
    }

    const Axis = enum {
        horizontal,
        vertical = 2,
    };
};

const Value = union(enum) {
    integer: i64,
    none,
};

const Color = enum(u8) {
    red,
    green = 4,
    _,
};

const Handle = opaque {};

pub const OpenError = error {
    AccessDenied,
    NotFound,
};

const Word: type = usize;

pub inline fn max(comptime T: type, a: T, b: T) T {
    const chosen = if (a > b) a else b;
    return chosen;
}

extern fn malloc(size: usize) ?*anyopaque;

test "point distance" {
    const point = Point{ .x = 1 };
    _ = point;
}

test max {
    _ = max(u8, 1, 2);
}

test {
    _ = std;
}
