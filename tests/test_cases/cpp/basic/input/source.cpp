// code sourced from https://learnxinyminutes.com/c++
#include <iostream>
#include <string>

int main(int argc, char** argv) {
		return 0;
}

void fn();
void fn(void);

void print(char const* msg)
{
    printf("String %s\n", msg);
}

namespace Baz {
    void foo()
    {
        printf("foo\n");
    }
    void bar()
    {
        printf("bar\n");
    }
}

std::string bar = "bar";
const std::string& barRef = bar;

enum ECarTypes : uint8_t
{
    Sedan, // 0
    Hatchback, // 1
    SUV = 254, // 254
    Hybrid // 255
};

ECarTypes GetPreferredCarType()
{
    return ECarTypes::Hatchback;
}

class Dog {
    // Member variables and functions are private by default.
    std::string name;
    int weight;

// All members following this are public
// until "private:" or "protected:" is found.
public:

    // Default constructor
    Dog();

    // Member function declarations (implementations to follow)
    // Note that we use std::string here instead of placing
    // using namespace std;
    // above.
    // Never put a "using namespace" statement in a header.
    void setName(const std::string& dogsName);

    void setWeight(int dogsWeight);

    // Functions that do not modify the state of the object
    // should be marked as const.
    // This allows you to call them if given a const reference to the object.
    // Also note the functions must be explicitly declared as _virtual_
    // in order to be overridden in derived classes.
    // Functions are not virtual by default for performance reasons.
    virtual void print() const;

    // Functions can also be defined inside the class body.
    // Functions defined as such are automatically inlined.
    void bark() const { std::cout << name << " barks!\n"; }

    // Along with constructors, C++ provides destructors.
    // These are called when an object is deleted or falls out of scope.
    // This enables powerful paradigms such as RAII
    // (see below)
    // The destructor should be virtual if a class is to be derived from;
    // if it is not virtual, then the derived class' destructor will
    // not be called if the object is destroyed through a base-class reference
    // or pointer.
    virtual ~Dog();
}; 

Dog::Dog()
{
    std::cout << "A dog has been constructed\n";
}

void Dog::setName(const std::string& dogsName)
{
    name = dogsName;
}

void Dog::setWeight(int dogsWeight)
{
    weight = dogsWeight;
}

void Dog::print() const
{
    std::cout << "Dog is " << name << " and weighs " << weight << "kg\n";
}

Dog::~Dog()
{
    std::cout << "Goodbye " << name << '\n';
}

class OwnedDog : public Dog {

public:
    void setOwner(const std::string& dogsOwner);

    // Override the behavior of the print function for all OwnedDogs. See
    // https://en.wikipedia.org/wiki/Polymorphism_(computer_science)#Subtyping
    // for a more general introduction if you are unfamiliar with
    // subtype polymorphism.
    // The override keyword is optional but makes sure you are actually
    // overriding the method in a base class.
    void print() const override;

private:
    std::string owner;
};

// Meanwhile, in the corresponding .cpp file:

void OwnedDog::setOwner(const std::string& dogsOwner)
{
    owner = dogsOwner;
}

void OwnedDog::print() const
{
    Dog::print(); // Call the print function in the base Dog class
    std::cout << "Dog is owned by " << owner << '\n';
    // Prints "Dog is <name> and weights <weight>"
    //        "Dog is owned by <owner>"
}

using namespace std;

class Point {
public:
    // Member variables can be given default values in this manner.
    double x = 0;
    double y = 0;

    // Define a default constructor which does nothing
    // but initialize the Point to the default value (0, 0)
    Point() { };

    // The following syntax is known as an initialization list
    // and is the proper way to initialize class member values
    Point (double a, double b) :
        x(a),
        y(b)
    { /* Do nothing except initialize the values */ }

    // Overload the + operator.
    Point operator+(const Point& rhs) const;

    // Overload the += operator
    Point& operator+=(const Point& rhs);

    // It would also make sense to add the - and -= operators,
    // but we will skip those for brevity.
};

Point Point::operator+(const Point& rhs) const
{
    // Create a new point that is the sum of this one and rhs.
    return Point(x + rhs.x, y + rhs.y);
}

// It's good practice to return a reference to the leftmost variable of
// an assignment. `(a += b) == c` will work this way.
Point& Point::operator+=(const Point& rhs)
{
    x += rhs.x;
    y += rhs.y;

    // `this` is a pointer to the object, on which a method is called.
    return *this;
}

template<class T>
class Box {
public:
    // In this class, T can be used as any other type.
    void insert(const T&) { ... }
};

Box<Box<int>> boxOfBox;
boxOfBox.insert(intBox);

template<class T>
void barkThreeTimes(const T& input)
{
    input.bark();
    input.bark();
    input.bark();
}

#include <vector>
std::string val;
std::vector<string> my_vector; // initialize the vector
std::cin >> val;

my_vector.push_back(val); // will push the value of 'val' into vector ("array") my_vector
my_vector.push_back(val); // will push the value into the vector again (now having two elements)



