import Foundation

public let maxCount = 10
var name: String = "x"
let someDic: Dictionary<String, Double>

public class Vehicle: NSObject, Codable {
    let wheels: Int = 4
    private var speed: Double
    static let kind = "auto"

    var description: String {
        let prefix = "V-"
        return prefix + kind
    }

    public func accelerate(by amount: Double, since t: Int) -> Bool {
        let local = amount > 0
        return local
    }

    init(speed: Double) { self.speed = speed }
    deinit {}
    subscript(index: Int) -> Int { return index }

    enum Gear {
        case low, high
    }
}

struct Point<T: Numeric> {
    var x: T
    var y: T

    static func + (lhs: Point, rhs: Point) -> Point {
        return lhs
    }
}

enum Direction: String {
    case north, south
    case east = "E"
}

protocol Drawable: AnyObject {
    var area: Double { get }
    func draw() -> Void
    associatedtype Item
}

extension Point {
    func magnitude() -> T { return x }

    static prefix func +++ (operand: Point) -> Point {
        return operand
    }
}

actor Bank {
    var balance: Int = 0
}

typealias Handler = (Int) -> Void

prefix operator +++
