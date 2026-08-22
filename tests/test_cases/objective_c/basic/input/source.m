#import <Klass.h>
@import Foundation;

int sharedCounter = 0;
NSString *const KlassErrorDomain = @"com.example.klass";
static int fileLocalCounter = 42;

int addNumbers(int a, int b)
{
    return a + b;
}

static NSString *describeState(KlassState state)
{
    switch (state) {
        case KlassStateRunning:
            return @"running";
        default:
            return @"idle";
    }
}

int main(int argc, const char *argv[])
{
    int primitive = 1;
    Klass *object = [Klass sharedInstance];
    return 0;
}

@implementation Klass

+ (Klass *)KlassFromCount:(NSNumber *)defaultCount
{
    Klass *instance = [[Klass alloc] init];
    return instance;
}

+ (instancetype)sharedInstance
{
    static Klass *shared = nil;
    return shared;
}

- (void)reset
{
    count = 0;
    privateCount = 0;
}

- (NSNumber *)methodAParameterAsString:(NSString *)string andAParameterAsNumber:(NSNumber *)number
{
    return number;
}

- (void)requiredMethod
{
    sharedCounter += 1;
}

@end

@implementation Klass (Networking)

- (void)fetchWithURL:(NSString *)url completion:(KlassCompletionHandler)completion
{
    completion(0, @"done");
}

@end
