// std: c++26
#include "side.h"
// A limit by name (spec/digest.md §2.3, WP-156; WP-153 §13.2 B7 item 3):
// a pack index after substitution is absent at the pin (the field's type
// is the selected type), so a name in the index moves unseen in the
// instantiated field (`n::S::s` on side A, `Base::s` on side B under
// the rename): equal. The pattern spells the dependent name the same.
// Site 5 is reserved.
struct Base { static int s(); };
namespace n { struct S : Base {
#ifdef B
  static int h();
#else
  static int s();
#endif
}; }
template <class S, class... T> struct P { T...[sizeof(S::s()) - 4] f; };
P<n::S, n::S, long> p;
