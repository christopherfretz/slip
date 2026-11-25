#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
// WP-156 (S7, spec/digest.md §4.3): a lambda in an operand, numbered by
// the operand unit's parent; at namespace scope too.
decltype([] { return make(x); }()) l = 0;
using tag = decltype([] { return make(x); });
void h() { decltype([] { return make(x); }()) m = 0; (void)m; }
