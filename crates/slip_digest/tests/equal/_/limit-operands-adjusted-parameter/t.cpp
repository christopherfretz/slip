#include "side.h"
// A limit by name (spec/digest.md §2.3, WP-156; WP-153 §13.2 B7 item 2):
// an adjusted parameter of an instantiated function has no written type
// at the pin (`decltype(make(T())) q[2]` is `int *`), so the name in it
// moves unseen: equal. The pattern's operand is dependent (its own
// spellings hold it).
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
template <class T> void p(T, decltype(make(T())) q[2]);
void u() { p(lib::X(), nullptr); }
