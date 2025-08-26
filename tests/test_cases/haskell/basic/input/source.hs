-- Haskell source code sourced from https://learnxinyminutes.com/haskell/

double :: Integer -> Integer
double x = x * 2

data Color = Red | Blue | Green
say :: Color -> String
say Red   = "You are Red!"
say Blue  = "You are Blue!"
say Green = "You are Green!"

data Point = Point Float Float

data Point2D = CartesianPoint2D { x :: Float, y :: Float } 
             | PolarPoint2D { r :: Float, theta :: Float }

class Eq a where  
    (==) :: a -> a -> Bool  
    (/=) :: a -> a -> Bool  
    x == y = not (x /= y)  
    x /= y = not (x == y)

instance Eq TrafficLight where  
    Red == Red = True  
    Green == Green = True  
    Yellow == Yellow = True  
    _ == _ = False

