#include "side.h"
// A limit by name (WP-156 fix round 3, `spec/digest.md` §2.4 "Named
// limits that stay equal"): a type name alone in an operand Kiln does not
// keep, in a slot no trigger of L4 names. A destructor's template-id
// (`w.~Wr<n::S::s>()`) keeps the destructor only; the type name joins no
// expression (L3) and the function's unit has no trigger, so it moves
// unseen (`n::S::s` on side A, `Base::s` on side B under the rename;
// `unsigned long` both times): equal. Kiln closes the class (KT1 2.8).
struct Base { using s = unsigned long; };
namespace n { struct S : Base {
#ifdef B
  using h = unsigned long;
#else
  using s = unsigned long;
#endif
}; }
template <class T> struct Wr { T m; };
void d(Wr<unsigned long> &w) { w.~Wr<n::S::s>(); }
