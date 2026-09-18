#import <Foundation/Foundation.h>

#define KLASS_MAX_COUNT 100
#define KLASS_SQUARE(x) ((x) * (x))
#define KLASS_LOG_PREFIX @"[Klass] "
#define KLASS_SWAP(a, b) \
    do {                 \
        __typeof__(a) _t = (a); \
        (a) = (b);       \
        (b) = _t;        \
    } while (0)

typedef int KlassCounter;
typedef NSString *KlassName;
typedef void (*KlassCompletionHandler)(int status, NSString *message);

struct KlassPoint {
    int x;
    int y;
};

typedef struct {
    double latitude;
    double longitude;
} KlassCoordinate;

enum KlassDirection {
    KlassDirectionNorth,
    KlassDirectionSouth,
    KlassDirectionEast,
    KlassDirectionWest
};

enum {
    KlassFlagNone = 0,
    KlassFlagA = 1 << 0,
    KlassFlagB = 1 << 1
};

typedef NS_ENUM(NSInteger, KlassState) {
    KlassStateIdle,
    KlassStateRunning,
    KlassStateStopped
};

extern int sharedCounter;
extern NSString *const KlassErrorDomain;

@protocol FooProtocol <NSObject>
@required
- (void)requiredMethod;
@optional
- (NSString *)optionalDescription;
@property (nonatomic, readonly) NSInteger identifier;
@end

@interface Klass : NSObject <FooProtocol>
{
    int count;
    @private int privateCount;
    NSString *fooString;
}

@property int intProperty;
@property (readonly) NSString *readonlyString;
@property (nonatomic, strong) NSArray<NSString *> *items;
@property (getter=anotherPropertyGet, setter=anotherPropertySet:) int anotherProperty;

+ (Klass *)KlassFromCount:(NSNumber *)defaultCount;
+ (instancetype)sharedInstance;
- (void)reset;
- (NSNumber *)methodAParameterAsString:(NSString*)string andAParameterAsNumber:(NSNumber *)number;
@end

@interface Klass (Networking)
- (void)fetchWithURL:(NSString *)url completion:(KlassCompletionHandler)completion;
@end
