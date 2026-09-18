#define PAIR(first, second) ((first) + (second))

extern int externalCounter;
int add(int left, int right);

int add(int left, int right)
{
    int total = left + right;
done:
    return total;
}

@interface Example : NSObject
@property int value;
- (void)update:(int)value;
@end
