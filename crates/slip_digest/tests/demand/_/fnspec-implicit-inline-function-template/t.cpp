#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 1): Kiln marks a function
// template specialisation implicitly inline when it instantiates its
// definition from an inline pattern definition
// (kiln_sema/src/class/members.rs:902-907): `definition`'s, not the own
// unit's.
template <class T> int f(T a);
extern template int f<int>(int);
template <class T> inline int f(T a) { return a; }
#ifdef B
int use() { return f(1); }
#endif
