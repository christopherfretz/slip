#include "side.h"
// A limit by name (spec/digest.md §2.3, WP-156; WP-153 §13.2 B1): an
// array bound is a value only at the pin, in ordinary code and after
// substitution, so a name in it moves unseen (`n::S::s` on side A,
// `Base::s` on side B under the rename; `int` both times): equal.
// Site 3 is reserved until Kiln keeps the bound as written (KR9 §5.11).
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
