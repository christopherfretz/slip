#include "side.h"
// WP-156 fix round 3 (L1, mutation 6): an `operator+` named in an array bound, which no stream walks, binds to `lib::operator+` on side A (by argument-dependent lookup) and to `::operator+` on side B; both return `int`, so only the `Operator` entry (site 9) tells them apart: the operand unit changes.
namespace lib { struct X {}; }
#ifdef B
int operator+(lib::X, lib::X);
#else
namespace lib { int operator+(X, X); }
#endif
namespace n {
int buf[sizeof(lib::X{} + lib::X{})];
}
