#include "side.h"
// digest-kinds §11.3 (WP-066, Kiln WP-2504): an in-class `static const`
// member's value changes through a constant of another unit; its own
// declaration is the same text. At this pin the member has a `value`
// unit, which differs; at `913a490bb` it had none, and only `other::N`
// differed. The reader `r` reads the member by key and is equal at both.
namespace other {
#ifdef B
constexpr int N = 2;
#else
constexpr int N = 1;
#endif
}
struct S { static const int k = other::N; };
int r() { return S::k; }
