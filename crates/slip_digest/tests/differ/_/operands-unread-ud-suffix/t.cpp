#include "side.h"
// WP-156 fix round 3 (L1, mutation 6): a user-defined literal's suffix named in an array bound, which no stream walks, binds to `lib::operator""_w` on side A and to `::operator""_w` on side B; both return `int`, so only the `UdSuffix` entry (site 9) tells them apart: the operand unit changes.
#ifdef B
int operator""_w(unsigned long long);
#else
namespace lib { int operator""_w(unsigned long long); }
using namespace lib;
#endif
namespace n {
int buf[sizeof(1_w)];
}
