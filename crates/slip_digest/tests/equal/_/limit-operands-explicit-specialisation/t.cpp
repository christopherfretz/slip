#include "side.h"
// A limit by name (spec/digest.md §2.3, WP-156; WP-153 §13.2 S1, item
// 8): `explicit(e)` of a function template's specialisation is absent at
// the pin (Kiln keeps it on the pattern and on a member of an
// instantiated class, both walked by the unit's own stream), so a name
// in it that only the specialisation resolves moves unseen: equal.
struct Base { static int s(); };
namespace n { struct S : Base {
#ifdef B
  static int h();
#else
  static int s();
#endif
}; }
struct W { template <class T> explicit(sizeof(T::s()) != 1) W(T); };
W w{n::S()};
