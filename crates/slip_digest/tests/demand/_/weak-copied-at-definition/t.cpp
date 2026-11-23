#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 3): Kiln copies `weak` from
// the pattern's out-of-line definition when it instantiates the
// definition (kiln_sema/src/templ/class.rs:1603-1615).
template <class T> struct X { inline int f(); };
template <class T> __attribute__((weak)) inline int X<T>::f() { return 1; }
extern template struct X<int>;
#ifdef B
int use(X<int>& x) { return x.f(); }
#endif
