#include "side.h"
// A limit by name (spec/digest.md §2.3, WP-156; WP-153 §13.2 B1),
// narrowed by fix round 3 to a type name (acceptance item 3): an array
// bound is a value only at the pin, in ordinary code and after
// substitution, and a type name in it joins no expression (L3) and meets
// no trigger (L4), so it moves unseen (`n::S::s` on side A, `Base::s` on
// side B under the rename; `int` both times): equal. A function name in
// a bound is `differ/_/operands-array-bound-function-name`. Site 3 is
// reserved until Kiln keeps the bound as written (KR9 §5.11).
struct Base { using s = int; };
namespace n { struct S : Base {
#ifdef B
  using h = int;
#else
  using s = int;
#endif
}; }
char buf[sizeof(n::S::s)];
struct F { char m[sizeof(n::S::s)]; };
template <class T> struct A { char a[sizeof(typename T::s)]; };
A<n::S> a;
