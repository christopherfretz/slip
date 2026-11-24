#include "side.h"
// digest-kinds §11.3 (WP-066, Kiln WP-2504): the control: the in-class
// `static const` member's value unit is the same when only the spelling
// of the constant it reaches differs.
namespace other {
#ifdef B
constexpr int N = 0x1;
#else
constexpr int N = 1;
#endif
}
struct S { static const int k = other::N; };
int r() { return S::k; }
