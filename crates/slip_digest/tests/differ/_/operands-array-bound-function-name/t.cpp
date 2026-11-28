#include "side.h"
// WP-156 fix round 3 (L3, L5; acceptance item 3): the array bounds of
// `limit-operands-array-bound` before it was narrowed. A bound is a value
// only at the pin, so its expression is walked by no stream; the
// function it names is a site-9 entry of the unit's operand unit
// (`n::S::s` on side A, `Base::s` on side B under the rename): differ.
// The bound of the pattern `A<T>::a` is dependent and its name is the
// pattern's (L1).
struct Base { static int s(); };
namespace n { struct S : Base {
#ifdef B
  static int h();
#else
  static int s();
#endif
}; }
char buf[sizeof(n::S::s())];
struct F { char m[sizeof(n::S::s())]; };
template <class T> struct A { char a[sizeof(T::s())]; };
A<n::S> a;
